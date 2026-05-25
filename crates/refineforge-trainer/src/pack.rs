//! Deterministic dataset packing for SFT and causal-LM smoke training.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PackSftOptions {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub epochs: usize,
    pub seed: u64,
    pub max_seq_len: usize,
    pub world_size: usize,
    pub target_only: bool,
}

#[derive(Debug, Clone)]
pub struct CausalPreprocessOptions {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub chunk_len: usize,
    pub stride: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedRecord {
    pub id: String,
    pub split: String,
    pub token_start: usize,
    pub token_len: usize,
    pub context_tokens: usize,
    pub target_tokens: usize,
    pub trimmed_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenizerManifest {
    pub id: String,
    pub sha256: String,
    pub vocab_size: usize,
    pub vocab_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    pub schema_version: String,
    pub source_path: String,
    pub source_sha256: String,
    pub record_count: usize,
    pub target_only: bool,
    pub max_sequence_length: usize,
    pub seed: u64,
    pub epochs: usize,
    pub world_size: usize,
    pub total_tokens: usize,
    pub supervised_target_tokens: usize,
    pub context_tokens: usize,
    pub pack_sha256: String,
    pub tokenizer: TokenizerManifest,
    pub records_path: String,
    pub tokens_path: String,
    pub loss_mask_path: String,
    pub packing_report_path: String,
}

#[derive(Debug, Clone)]
pub struct LoadedPack {
    pub root: PathBuf,
    pub manifest: PackManifest,
    pub records: Vec<PackedRecord>,
    pub tokens: Vec<u32>,
    pub loss_mask: Vec<u8>,
}

#[derive(Debug, Clone)]
struct SftRow {
    id: String,
    split: String,
    prompt: String,
    target: String,
}

#[derive(Debug, Clone, Default, Serialize)]
struct MultipackRank {
    rank: usize,
    token_count: usize,
    record_ids: Vec<String>,
}

pub fn pack_sft(opts: &PackSftOptions) -> Result<PackManifest> {
    if opts.epochs == 0 {
        anyhow::bail!("--epochs must be greater than 0");
    }
    if opts.max_seq_len < 2 {
        anyhow::bail!("--max-seq-len must be at least 2");
    }
    if opts.world_size == 0 {
        anyhow::bail!("--world-size must be greater than 0");
    }

    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("creating {}", opts.out_dir.display()))?;
    let input_bytes = read_maybe_zstd(&opts.input)?;
    let source_sha256 = hex_sha256(&input_bytes);
    let rows = parse_sft_rows(&input_bytes)?;
    if rows.is_empty() {
        anyhow::bail!("SFT pack input has no rows: {}", opts.input.display());
    }

    let mut tokenizer = StableTokenizer::new();
    let mut all_tokens = Vec::<u32>::new();
    let mut loss_mask = Vec::<u8>::new();
    let mut records = Vec::<PackedRecord>::new();
    let mut total_target_tokens = 0usize;
    let mut total_context_tokens = 0usize;
    let mut total_trimmed = 0usize;

    for row in &rows {
        let mut prompt_tokens = tokenizer.encode(&row.prompt);
        let mut target_tokens = tokenizer.encode(&row.target);
        if target_tokens.is_empty() {
            target_tokens.push(tokenizer.token_id("<empty-target>"));
        }
        let original_len = prompt_tokens.len() + target_tokens.len();
        if original_len > opts.max_seq_len {
            let target_keep = target_tokens.len().min(opts.max_seq_len);
            let prompt_keep = opts.max_seq_len.saturating_sub(target_keep);
            if prompt_tokens.len() > prompt_keep {
                let start = prompt_tokens.len() - prompt_keep;
                prompt_tokens = prompt_tokens[start..].to_vec();
            }
            if target_tokens.len() > target_keep {
                target_tokens.truncate(target_keep);
            }
        }
        let trimmed = original_len.saturating_sub(prompt_tokens.len() + target_tokens.len());
        total_trimmed += trimmed;

        let token_start = all_tokens.len();
        all_tokens.extend(prompt_tokens.iter().copied());
        all_tokens.extend(target_tokens.iter().copied());
        if opts.target_only {
            loss_mask.extend(std::iter::repeat_n(0u8, prompt_tokens.len()));
        } else {
            loss_mask.extend(std::iter::repeat_n(1u8, prompt_tokens.len()));
        }
        loss_mask.extend(std::iter::repeat_n(1u8, target_tokens.len()));
        total_context_tokens += prompt_tokens.len();
        total_target_tokens += target_tokens.len();

        records.push(PackedRecord {
            id: row.id.clone(),
            split: row.split.clone(),
            token_start,
            token_len: prompt_tokens.len() + target_tokens.len(),
            context_tokens: prompt_tokens.len(),
            target_tokens: target_tokens.len(),
            trimmed_tokens: trimmed,
        });
    }

    let vocab_json = tokenizer.vocab_json();
    let tokenizer_sha256 = hex_sha256(vocab_json.to_string().as_bytes());
    write_json_pretty(&opts.out_dir.join("tokenizer.json"), &vocab_json)?;
    write_u32_le(&opts.out_dir.join("tokens.bin"), &all_tokens)?;
    std::fs::write(opts.out_dir.join("loss-mask.bin"), &loss_mask)?;
    write_json_pretty(&opts.out_dir.join("records.json"), &records)?;

    for epoch in 0..opts.epochs {
        let order = shuffled_indices(records.len(), opts.seed ^ epoch as u64);
        write_json_pretty(
            &opts.out_dir.join(format!("epoch-{epoch:03}-shuffle.json")),
            &json!({
                "schema_version": "refineforge-sft-shuffle-v1",
                "epoch": epoch,
                "seed": opts.seed,
                "order": order
            }),
        )?;
    }

    let rank_plan = multipack_rank_plan(&records, opts.world_size);
    let rank_balance: Vec<usize> = rank_plan.iter().map(|rank| rank.token_count).collect();
    let planned_capacity = rank_balance
        .iter()
        .map(|tokens| tokens.div_ceil(opts.max_seq_len) * opts.max_seq_len)
        .sum::<usize>()
        .max(1);
    let slot_utilization = all_tokens.len() as f64 / planned_capacity as f64;
    write_json_pretty(
        &opts.out_dir.join("multipack-plan.json"),
        &json!({
            "schema_version": "refineforge-multipack-plan-v1",
            "world_size": opts.world_size,
            "max_sequence_length": opts.max_seq_len,
            "ranks": rank_plan
        }),
    )?;
    write_json_pretty(
        &opts.out_dir.join("packing_report.json"),
        &json!({
            "schema_version": "refineforge-packing-report-v1",
            "total_tokens": all_tokens.len(),
            "supervised_target_tokens": total_target_tokens,
            "context_tokens": total_context_tokens,
            "max_sequence_length": opts.max_seq_len,
            "slot_utilization": slot_utilization,
            "rank_balance": rank_balance,
            "dropped_samples": 0,
            "trimmed_tokens": total_trimmed,
            "target_only": opts.target_only
        }),
    )?;

    let pack_sha256 = hash_pack_outputs(
        &opts.out_dir,
        &[
            "tokens.bin",
            "loss-mask.bin",
            "records.json",
            "packing_report.json",
            "multipack-plan.json",
            "tokenizer.json",
        ],
    )?;
    let manifest = PackManifest {
        schema_version: "refineforge-sft-pack-v1".to_string(),
        source_path: opts.input.display().to_string(),
        source_sha256,
        record_count: records.len(),
        target_only: opts.target_only,
        max_sequence_length: opts.max_seq_len,
        seed: opts.seed,
        epochs: opts.epochs,
        world_size: opts.world_size,
        total_tokens: all_tokens.len(),
        supervised_target_tokens: total_target_tokens,
        context_tokens: total_context_tokens,
        pack_sha256,
        tokenizer: TokenizerManifest {
            id: "refineforge-stable-tokenizer-v1".to_string(),
            sha256: tokenizer_sha256,
            vocab_size: tokenizer.vocab.len(),
            vocab_path: "tokenizer.json".to_string(),
        },
        records_path: "records.json".to_string(),
        tokens_path: "tokens.bin".to_string(),
        loss_mask_path: "loss-mask.bin".to_string(),
        packing_report_path: "packing_report.json".to_string(),
    };
    write_json_pretty(&opts.out_dir.join("pack-manifest.json"), &manifest)?;
    Ok(manifest)
}

pub fn causal_lm_preprocess(opts: &CausalPreprocessOptions) -> Result<Value> {
    if opts.chunk_len < 2 {
        anyhow::bail!("--chunk-len must be at least 2");
    }
    if opts.stride == 0 {
        anyhow::bail!("--stride must be greater than 0");
    }
    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("creating {}", opts.out_dir.display()))?;
    let input_bytes = read_maybe_zstd(&opts.input)?;
    let input_sha256 = hex_sha256(&input_bytes);
    let text =
        String::from_utf8(input_bytes).context("causal-LM input is not UTF-8 after decode")?;
    let mut tokenizer = StableTokenizer::new();
    let mut tokens = Vec::<u32>::new();
    let mut document_count = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value =
            serde_json::from_str(line).with_context(|| "causal-LM input line is not JSON")?;
        let doc = value
            .get("text")
            .and_then(Value::as_str)
            .or_else(|| value.as_str())
            .context("causal-LM row must contain text")?;
        tokens.extend(tokenizer.encode(doc));
        tokens.push(tokenizer.token_id("<eod>"));
        document_count += 1;
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start + 1 < tokens.len() {
        let end = (start + opts.chunk_len).min(tokens.len());
        chunks.push(json!({
            "start": start,
            "end": end,
            "token_count": end - start
        }));
        if end == tokens.len() {
            break;
        }
        start = start.saturating_add(opts.stride);
    }

    write_u32_le(&opts.out_dir.join("tokens.bin"), &tokens)?;
    let vocab_json = tokenizer.vocab_json();
    write_json_pretty(&opts.out_dir.join("tokenizer.json"), &vocab_json)?;
    write_json_pretty(&opts.out_dir.join("chunks.json"), &chunks)?;
    let output_sha256 = hash_pack_outputs(
        &opts.out_dir,
        &["tokens.bin", "tokenizer.json", "chunks.json"],
    )?;
    let manifest = json!({
        "schema_version": "refineforge-causal-lm-pack-v1",
        "source_path": opts.input.display().to_string(),
        "input_sha256": input_sha256,
        "document_count": document_count,
        "token_count": tokens.len(),
        "chunk_length": opts.chunk_len,
        "stride": opts.stride,
        "chunk_count": chunks.len(),
        "output_sha256": output_sha256,
        "tokenizer": {
            "id": "refineforge-stable-tokenizer-v1",
            "sha256": hex_sha256(vocab_json.to_string().as_bytes()),
            "vocab_size": tokenizer.vocab.len(),
            "vocab_path": "tokenizer.json"
        },
        "tokens_path": "tokens.bin",
        "chunks_path": "chunks.json"
    });
    write_json_pretty(&opts.out_dir.join("causal-lm-manifest.json"), &manifest)?;
    Ok(manifest)
}

pub fn load_pack(path: &Path) -> Result<LoadedPack> {
    let root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };
    let manifest_path = if path.is_dir() {
        path.join("pack-manifest.json")
    } else {
        path.to_path_buf()
    };
    let manifest: PackManifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parsing {}", manifest_path.display()))?;
    let records: Vec<PackedRecord> = serde_json::from_str(
        &std::fs::read_to_string(root.join(&manifest.records_path))
            .with_context(|| format!("reading {}", root.join(&manifest.records_path).display()))?,
    )?;
    let tokens = read_u32_le(&root.join(&manifest.tokens_path))?;
    let loss_mask = std::fs::read(root.join(&manifest.loss_mask_path))?;
    if tokens.len() != loss_mask.len() {
        anyhow::bail!(
            "pack token/mask length mismatch: {} tokens vs {} mask entries",
            tokens.len(),
            loss_mask.len()
        );
    }
    Ok(LoadedPack {
        root,
        manifest,
        records,
        tokens,
        loss_mask,
    })
}

fn parse_sft_rows(bytes: &[u8]) -> Result<Vec<SftRow>> {
    let text = String::from_utf8(bytes.to_vec()).context("SFT input is not UTF-8 after decode")?;
    let mut rows = Vec::new();
    for (line_idx, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("parsing SFT row {}", line_idx + 1))?;
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("row-{}", line_idx + 1));
        let prompt = value
            .get("prompt")
            .and_then(Value::as_str)
            .context("SFT row missing prompt")?
            .to_string();
        let response = value
            .get("response")
            .and_then(Value::as_str)
            .context("SFT row missing response")?;
        let target = target_text_from_response(response)
            .with_context(|| format!("extracting target text for SFT row {id}"))?;
        let split = split_for(&value).unwrap_or_else(|| "train".to_string());
        rows.push(SftRow {
            id,
            split,
            prompt,
            target,
        });
    }
    Ok(rows)
}

fn target_text_from_response(response: &str) -> Result<String> {
    let value: Value = serde_json::from_str(response).context("response is not JSON")?;
    let patch = value.get("patch").unwrap_or(&value);
    let new_text = patch
        .get("new_text")
        .and_then(Value::as_str)
        .context("response patch is missing new_text")?;
    let rationale = patch.get("rationale").and_then(Value::as_str).unwrap_or("");
    Ok(format!("{new_text}\n{rationale}"))
}

fn split_for(value: &Value) -> Option<String> {
    value
        .get("split")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("metadata")
                .and_then(|metadata| metadata.get("split"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

#[derive(Debug, Clone)]
struct StableTokenizer {
    vocab: BTreeMap<String, u32>,
}

impl StableTokenizer {
    fn new() -> Self {
        let mut vocab = BTreeMap::new();
        vocab.insert("<pad>".to_string(), 0);
        vocab.insert("<unk>".to_string(), 1);
        Self { vocab }
    }

    fn token_id(&mut self, token: &str) -> u32 {
        if let Some(id) = self.vocab.get(token) {
            return *id;
        }
        let id = self.vocab.len() as u32;
        self.vocab.insert(token.to_string(), id);
        id
    }

    fn encode(&mut self, text: &str) -> Vec<u32> {
        lex(text)
            .into_iter()
            .map(|token| self.token_id(&token))
            .collect()
    }

    fn vocab_json(&self) -> Value {
        json!({
            "schema_version": "refineforge-stable-tokenizer-v1",
            "kind": "unicode-wordpunct",
            "vocab": self.vocab
        })
    }
}

fn lex(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            if !ch.is_whitespace() {
                tokens.push(ch.to_string());
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn shuffled_indices(len: usize, seed: u64) -> Vec<usize> {
    let mut values: Vec<usize> = (0..len).collect();
    let mut rng = XorShift64(seed.max(1));
    for i in (1..values.len()).rev() {
        let j = (rng.next() as usize) % (i + 1);
        values.swap(i, j);
    }
    values
}

fn multipack_rank_plan(records: &[PackedRecord], world_size: usize) -> Vec<MultipackRank> {
    let mut ranks: Vec<MultipackRank> = (0..world_size)
        .map(|rank| MultipackRank {
            rank,
            ..MultipackRank::default()
        })
        .collect();
    let mut order: Vec<&PackedRecord> = records.iter().collect();
    order.sort_by_key(|record| std::cmp::Reverse(record.token_len));
    for record in order {
        let idx = ranks
            .iter()
            .enumerate()
            .min_by_key(|(_, rank)| rank.token_count)
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        ranks[idx].token_count += record.token_len;
        ranks[idx].record_ids.push(record.id.clone());
    }
    ranks
}

#[derive(Debug, Clone)]
struct XorShift64(u64);

impl XorShift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

pub fn read_u32_le(path: &Path) -> Result<Vec<u32>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.len() % 4 != 0 {
        anyhow::bail!("{} byte length is not divisible by 4", path.display());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn write_u32_le(path: &Path, values: &[u32]) -> Result<()> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend(value.to_le_bytes());
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

fn read_maybe_zstd(path: &Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zst"))
    {
        let mut decoder = zstd::stream::read::Decoder::new(&bytes[..])
            .with_context(|| format!("opening zstd stream {}", path.display()))?;
        let mut decoded = Vec::new();
        decoder
            .read_to_end(&mut decoded)
            .with_context(|| format!("decoding zstd stream {}", path.display()))?;
        Ok(decoded)
    } else {
        Ok(bytes)
    }
}

fn write_json_pretty<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", path.display()))
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_pack_outputs(root: &Path, relative_paths: &[&str]) -> Result<String> {
    let mut hasher = Sha256::new();
    for relative in relative_paths {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        let bytes = std::fs::read(root.join(relative))
            .with_context(|| format!("reading {}", root.join(relative).display()))?;
        hasher.update(bytes);
        hasher.update([0xff]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
