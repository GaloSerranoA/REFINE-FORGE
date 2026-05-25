use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: &str = "refineforge-memory-v1";
pub const TRUST_EFFECT_NONE: &str = "none";

const VALID_AGENTS: &[&str] = &["lean", "devops", "train", "kernel", "run_all"];
const VALID_KINDS: &[&str] = &[
    "preference",
    "citation",
    "evidence_index",
    "handoff",
    "claim_note",
    "blocker",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRecord {
    pub schema_version: String,
    pub id: String,
    pub agent: String,
    pub target: String,
    pub kind: String,
    pub content: String,
    pub source_path: Option<String>,
    pub source_sha256: Option<String>,
    pub created_at: String,
    pub trust_effect: String,
}

#[derive(Debug, Clone)]
pub struct AddOptions {
    pub store: Option<PathBuf>,
    pub agent: String,
    pub target: String,
    pub kind: String,
    pub content: String,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub store: Option<PathBuf>,
    pub agent: Option<String>,
    pub target: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportSummary {
    pub schema_version: String,
    pub store_path: String,
    pub imported: usize,
    pub skipped_duplicates: usize,
}

pub fn default_store(root: &Path) -> PathBuf {
    root.join(".refineforge")
        .join("memory")
        .join("records.jsonl")
}

pub fn add_with_store(root: &Path, opts: AddOptions) -> Result<MemoryRecord> {
    let store = store_path(root, opts.store.as_ref())?;
    let record = build_record(root, opts)?;
    append_if_missing(&store, &record)?;
    Ok(record)
}

pub fn list(root: &Path, opts: ListOptions) -> Result<Vec<MemoryRecord>> {
    let store = store_path(root, opts.store.as_ref())?;
    let mut records = read_records_if_exists(&store)?;
    records.retain(|record| {
        opts.agent
            .as_deref()
            .is_none_or(|agent| record.agent == agent)
            && opts
                .target
                .as_deref()
                .is_none_or(|target| record.target == target)
            && opts.kind.as_deref().is_none_or(|kind| record.kind == kind)
    });
    records.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(records)
}

pub fn import_jsonl(root: &Path, from: &Path, store: Option<&PathBuf>) -> Result<ImportSummary> {
    let store = store_path(root, store)?;
    let incoming = read_records(from).with_context(|| format!("reading {}", from.display()))?;
    let existing = read_records_if_exists(&store)?;
    let mut ids: BTreeSet<String> = existing.into_iter().map(|record| record.id).collect();

    if let Some(parent) = store.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&store)
        .with_context(|| format!("opening {}", store.display()))?;

    let mut imported = 0;
    let mut skipped_duplicates = 0;
    for record in incoming {
        validate_record(&record)?;
        if ids.insert(record.id.clone()) {
            writeln!(file, "{}", serde_json::to_string(&record)?)
                .with_context(|| format!("writing {}", store.display()))?;
            imported += 1;
        } else {
            skipped_duplicates += 1;
        }
    }

    Ok(ImportSummary {
        schema_version: SCHEMA_VERSION.to_string(),
        store_path: store.display().to_string(),
        imported,
        skipped_duplicates,
    })
}

pub fn export_jsonl(root: &Path, out: &Path, store: Option<&PathBuf>) -> Result<ImportSummary> {
    let store = store_path(root, store)?;
    let records = read_records_if_exists(&store)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    for record in &records {
        validate_record(record)?;
        writeln!(file, "{}", serde_json::to_string(record)?)
            .with_context(|| format!("writing {}", out.display()))?;
    }
    Ok(ImportSummary {
        schema_version: SCHEMA_VERSION.to_string(),
        store_path: out.display().to_string(),
        imported: records.len(),
        skipped_duplicates: 0,
    })
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn build_record(root: &Path, opts: AddOptions) -> Result<MemoryRecord> {
    validate_agent(&opts.agent)?;
    validate_kind(&opts.kind)?;
    if opts.target.trim().is_empty() {
        bail!("memory target cannot be empty");
    }
    if opts.content.trim().is_empty() {
        bail!("memory content cannot be empty");
    }

    let (source_path, source_sha256) = match opts.source_path {
        Some(path) => {
            let resolved = resolve_under_root(root, &path);
            if !resolved.is_file() {
                bail!(
                    "memory source path must be an existing file: {}",
                    resolved.display()
                );
            }
            (
                Some(path.display().to_string()),
                Some(
                    hash_file(&resolved)
                        .with_context(|| format!("hashing memory source {}", resolved.display()))?,
                ),
            )
        }
        None => (None, None),
    };

    let id = record_id(
        &opts.agent,
        &opts.target,
        &opts.kind,
        &opts.content,
        source_path.as_deref(),
        source_sha256.as_deref(),
    );

    Ok(MemoryRecord {
        schema_version: SCHEMA_VERSION.to_string(),
        id,
        agent: opts.agent,
        target: opts.target,
        kind: opts.kind,
        content: opts.content,
        source_path,
        source_sha256,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        trust_effect: TRUST_EFFECT_NONE.to_string(),
    })
}

fn record_id(
    agent: &str,
    target: &str,
    kind: &str,
    content: &str,
    source_path: Option<&str>,
    source_sha256: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        SCHEMA_VERSION,
        agent,
        target,
        kind,
        content,
        source_path.unwrap_or(""),
        source_sha256.unwrap_or(""),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("rfmem:{}", hex::encode(hasher.finalize()))
}

fn append_if_missing(store: &Path, record: &MemoryRecord) -> Result<()> {
    validate_record(record)?;
    let existing = read_records_if_exists(store)?;
    if existing.iter().any(|existing| existing.id == record.id) {
        return Ok(());
    }
    if let Some(parent) = store.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(store)
        .with_context(|| format!("opening {}", store.display()))?;
    writeln!(file, "{}", serde_json::to_string(record)?)
        .with_context(|| format!("writing {}", store.display()))?;
    Ok(())
}

fn read_records_if_exists(path: &Path) -> Result<Vec<MemoryRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_records(path)
}

fn read_records(path: &Path) -> Result<Vec<MemoryRecord>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: MemoryRecord = serde_json::from_str(line)
            .with_context(|| format!("parsing {} line {}", path.display(), index + 1))?;
        validate_record(&record)
            .with_context(|| format!("validating {} line {}", path.display(), index + 1))?;
        records.push(record);
    }
    Ok(records)
}

fn validate_record(record: &MemoryRecord) -> Result<()> {
    if record.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported memory schema_version {:?}, expected {SCHEMA_VERSION:?}",
            record.schema_version
        );
    }
    if record.trust_effect != TRUST_EFFECT_NONE {
        bail!(
            "memory trust_effect must be {TRUST_EFFECT_NONE:?}, got {:?}",
            record.trust_effect
        );
    }
    validate_agent(&record.agent)?;
    validate_kind(&record.kind)?;
    validate_hash_id(record)?;
    if let Some(source_sha256) = &record.source_sha256 {
        validate_lower_hex_64("source_sha256", source_sha256)?;
    }
    if record.target.trim().is_empty() {
        bail!("memory target cannot be empty");
    }
    if record.content.trim().is_empty() {
        bail!("memory content cannot be empty");
    }
    Ok(())
}

fn validate_hash_id(record: &MemoryRecord) -> Result<()> {
    let hash = record
        .id
        .strip_prefix("rfmem:")
        .ok_or_else(|| anyhow::anyhow!("memory id must start with rfmem:"))?;
    validate_lower_hex_64("id", hash)?;
    let expected = record_id(
        &record.agent,
        &record.target,
        &record.kind,
        &record.content,
        record.source_path.as_deref(),
        record.source_sha256.as_deref(),
    );
    if record.id != expected {
        bail!("memory id does not match record content");
    }
    Ok(())
}

fn validate_lower_hex_64(field: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        bail!("memory {field} must be 64 lowercase hex characters");
    }
    Ok(())
}

fn validate_agent(agent: &str) -> Result<()> {
    if VALID_AGENTS.contains(&agent) {
        Ok(())
    } else {
        bail!(
            "invalid memory agent {agent:?}; expected one of {}",
            VALID_AGENTS.join(", ")
        )
    }
}

fn validate_kind(kind: &str) -> Result<()> {
    if VALID_KINDS.contains(&kind) {
        Ok(())
    } else {
        bail!(
            "invalid memory kind {kind:?}; expected one of {}",
            VALID_KINDS.join(", ")
        )
    }
}

fn store_path(root: &Path, store: Option<&PathBuf>) -> Result<PathBuf> {
    match store {
        Some(path) if path.is_absolute() => Ok(path.clone()),
        Some(path) => Ok(root.join(path)),
        None => Ok(default_store(root)),
    }
}

fn resolve_under_root(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_ids_ignore_created_at() {
        let a = record_id("train", "helyx", "preference", "use dry runs", None, None);
        let b = record_id("train", "helyx", "preference", "use dry runs", None, None);
        assert_eq!(a, b);
        assert_ne!(
            a,
            record_id("train", "helyx", "preference", "use live runs", None, None)
        );
    }
}
