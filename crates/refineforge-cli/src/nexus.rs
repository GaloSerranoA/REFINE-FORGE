use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{claim, sorry_gate};

const SYSTEM_NAME: &str = "InmortalProof";
const REPORT_SCHEMA: &str = "refineforge-inmortalproof-report-v1";
const PUBLIC_CLAIM: &str = "inmortalproof_receipt_only_not_alphaproof";
const RATER_VERSION: &str = "inmortalproof-rater-v1";

#[derive(Debug, Clone)]
pub struct NexusRunOptions {
    pub out_dir: PathBuf,
    pub episodes: usize,
    pub population_limit: usize,
    pub emit_json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusReport {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub system_name: String,
    pub status: String,
    pub public_claim: String,
    pub claim_id: String,
    pub claim_path: PathBuf,
    pub lean_file: PathBuf,
    pub lean_module: String,
    pub theorems: Vec<String>,
    pub editable_regions: Vec<EditableRegion>,
    pub integrity: IntegrityReceipt,
    pub episodes: Vec<NexusEpisode>,
    pub population: Vec<NexusSketch>,
    pub goal_cache: Vec<GoalCacheEntry>,
    pub rater: RaterReceipt,
    pub blockers: Vec<String>,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditableRegion {
    pub id: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub body_sha256: String,
    pub marker_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReceipt {
    pub source_sha256: String,
    pub protected_sha256: String,
    pub protected_region_count: usize,
    pub policy_ok: bool,
    pub sorry_count: usize,
    pub admit_count: usize,
    pub axiom_count: usize,
    pub theorem_statement_changed: bool,
    pub lean_check: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusEpisode {
    pub index: usize,
    pub parent_sketch_id: String,
    pub mode: String,
    pub search_replace_calls: usize,
    pub alphaproof_queries: usize,
    pub edits_accepted: usize,
    pub feedback: Vec<String>,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusSketch {
    pub id: String,
    pub parent_id: Option<String>,
    pub source_sha256: String,
    pub protected_sha256: String,
    pub region_count: usize,
    pub sorry_count: usize,
    pub validation_status: String,
    pub elo: f64,
    pub visits: usize,
    pub selection: SelectionReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionReceipt {
    pub algorithm: String,
    pub q: f64,
    pub exploration_c: f64,
    pub visit_count: usize,
    pub total_visits: usize,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalCacheEntry {
    pub theorem: String,
    pub goal_id: String,
    pub status: String,
    pub source: String,
    pub protected_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaterReceipt {
    pub rubric_version: String,
    pub criteria: Vec<String>,
}

#[derive(Debug)]
struct RegionScan {
    regions: Vec<EditableRegion>,
    protected_source: String,
    blockers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKind {
    Block,
    Value,
}

impl RegionKind {
    fn as_str(self) -> &'static str {
        match self {
            RegionKind::Block => "block",
            RegionKind::Value => "value",
        }
    }

    fn start_marker(self) -> &'static str {
        match self {
            RegionKind::Block => "-- EVOLVE-BLOCK-START",
            RegionKind::Value => "-- EVOLVE-VALUE-START",
        }
    }

    fn end_marker(self) -> &'static str {
        match self {
            RegionKind::Block => "-- EVOLVE-BLOCK-END",
            RegionKind::Value => "-- EVOLVE-VALUE-END",
        }
    }
}

pub fn run(root: &Path, claim_id: &str, opts: NexusRunOptions) -> Result<()> {
    let report = build_report(root, claim_id, &opts)?;
    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("creating {}", opts.out_dir.display()))?;

    write_report_artifacts(&opts.out_dir, &report)?;

    if opts.emit_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "InmortalProof proof-search receipt: {} ({})",
            opts.out_dir.join("inmortal-proof-report.md").display(),
            report.status
        );
    }
    Ok(())
}

fn build_report(root: &Path, claim_id: &str, opts: &NexusRunOptions) -> Result<NexusReport> {
    let (claim_path, loaded_claim) = claim::load(root, claim_id)?;
    let lean_path = root.join(&loaded_claim.lean.file);
    let source = std::fs::read_to_string(&lean_path)
        .with_context(|| format!("reading Lean file {}", lean_path.display()))?;

    let scan = scan_evolve_regions(&source);
    let gate = sorry_gate::check(&source, &loaded_claim.policy);

    let source_sha256 = sha256_hex(source.as_bytes());
    let protected_sha256 = sha256_hex(scan.protected_source.as_bytes());
    let mut blockers = scan.blockers;

    if scan.regions.is_empty() {
        blockers.push("no EVOLVE-BLOCK or EVOLVE-VALUE markers found in Lean source".to_string());
    }
    if !gate.ok {
        blockers.extend(gate.notes.iter().cloned());
    }
    if loaded_claim.lean.theorems.is_empty() {
        blockers.push("claim has no Lean theorems for Nexus goal caching".to_string());
    }
    if opts.population_limit == 0 {
        blockers.push("population_limit must be greater than zero".to_string());
    }

    let status = if blockers.is_empty() {
        "ready_for_search"
    } else {
        "blocked"
    }
    .to_string();

    let integrity = IntegrityReceipt {
        source_sha256: source_sha256.clone(),
        protected_sha256: protected_sha256.clone(),
        protected_region_count: scan.regions.len(),
        policy_ok: gate.ok,
        sorry_count: gate.sorry_count,
        admit_count: gate.admit_count,
        axiom_count: gate.axiom_count,
        theorem_statement_changed: false,
        lean_check: "not_run".to_string(),
    };

    let population = vec![initial_sketch(
        &source_sha256,
        &protected_sha256,
        scan.regions.len(),
        gate.sorry_count,
        &status,
    )];
    let episodes = build_episodes(opts.episodes);
    let goal_cache = loaded_claim
        .lean
        .theorems
        .iter()
        .enumerate()
        .map(|(idx, theorem)| GoalCacheEntry {
            theorem: theorem.clone(),
            goal_id: format!("goal-{:04}", idx + 1),
            status: if gate.sorry_count == 0 && gate.admit_count == 0 {
                "unresolved"
            } else {
                "blocked_by_policy"
            }
            .to_string(),
            source: "claim_theorem".to_string(),
            protected_sha256: protected_sha256.clone(),
        })
        .collect();

    Ok(NexusReport {
        schema_version: REPORT_SCHEMA.to_string(),
        generated_at: Utc::now(),
        system_name: SYSTEM_NAME.to_string(),
        status,
        public_claim: PUBLIC_CLAIM.to_string(),
        claim_id: loaded_claim.claim_id,
        claim_path,
        lean_file: lean_path,
        lean_module: loaded_claim.lean.module,
        theorems: loaded_claim.lean.theorems,
        editable_regions: scan.regions,
        integrity,
        episodes,
        population,
        goal_cache,
        rater: RaterReceipt {
            rubric_version: RATER_VERSION.to_string(),
            criteria: vec![
                "Lean policy gate must remain clean".to_string(),
                "protected source hash must remain stable outside EVOLVE regions".to_string(),
                "candidate sketches must carry deterministic selection evidence".to_string(),
                "accepted edits must be replayable from JSONL receipts".to_string(),
            ],
        },
        blockers,
        boundary: "InmortalProof is Refine-Forge's deterministic proof-search substrate and receipt layer. It does not claim AlphaProof, Gemini, proprietary model access, or a complete proof."
            .to_string(),
    })
}

fn scan_evolve_regions(source: &str) -> RegionScan {
    let lines: Vec<&str> = source.lines().collect();
    let mut regions = Vec::new();
    let mut blockers = Vec::new();
    let mut active: Option<(RegionKind, usize)> = None;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let marker = line.trim();
        match marker {
            "-- EVOLVE-BLOCK-START" => start_region(
                &mut active,
                &mut blockers,
                RegionKind::Block,
                line_no,
                &regions,
            ),
            "-- EVOLVE-VALUE-START" => start_region(
                &mut active,
                &mut blockers,
                RegionKind::Value,
                line_no,
                &regions,
            ),
            "-- EVOLVE-BLOCK-END" => end_region(
                &mut active,
                &mut blockers,
                RegionKind::Block,
                line_no,
                &lines,
                &mut regions,
            ),
            "-- EVOLVE-VALUE-END" => end_region(
                &mut active,
                &mut blockers,
                RegionKind::Value,
                line_no,
                &lines,
                &mut regions,
            ),
            _ => {}
        }
    }

    if let Some((kind, start_line)) = active {
        blockers.push(format!(
            "unterminated {} marker beginning at line {}",
            kind.as_str(),
            start_line
        ));
    }

    let protected_source = build_protected_source(source, &lines, &regions);
    RegionScan {
        regions,
        protected_source,
        blockers,
    }
}

fn start_region(
    active: &mut Option<(RegionKind, usize)>,
    blockers: &mut Vec<String>,
    kind: RegionKind,
    line_no: usize,
    regions: &[EditableRegion],
) {
    if let Some((current_kind, start_line)) = active {
        blockers.push(format!(
            "nested {} marker at line {} while {} marker from line {} is still open",
            kind.as_str(),
            line_no,
            current_kind.as_str(),
            start_line
        ));
        return;
    }
    if regions
        .last()
        .is_some_and(|region| line_no <= region.end_line)
    {
        blockers.push(format!(
            "{} marker at line {} overlaps a previous EVOLVE region",
            kind.as_str(),
            line_no
        ));
        return;
    }
    *active = Some((kind, line_no));
}

fn end_region(
    active: &mut Option<(RegionKind, usize)>,
    blockers: &mut Vec<String>,
    kind: RegionKind,
    line_no: usize,
    lines: &[&str],
    regions: &mut Vec<EditableRegion>,
) {
    let Some((current_kind, start_line)) = active.take() else {
        blockers.push(format!(
            "{} end marker at line {} has no matching start marker",
            kind.as_str(),
            line_no
        ));
        return;
    };

    if current_kind != kind {
        blockers.push(format!(
            "{} end marker at line {} closes {} marker from line {}",
            kind.as_str(),
            line_no,
            current_kind.as_str(),
            start_line
        ));
        return;
    }

    let body_start = start_line;
    let body_end = line_no.saturating_sub(1);
    let body = if body_start < body_end {
        lines[body_start..body_end].join("\n")
    } else {
        String::new()
    };
    let marker_text = format!("{}\n{}", kind.start_marker(), kind.end_marker());
    let id = format!("region-{:04}", regions.len() + 1);
    regions.push(EditableRegion {
        id,
        kind: kind.as_str().to_string(),
        start_line,
        end_line: line_no,
        body_sha256: sha256_hex(body.as_bytes()),
        marker_sha256: sha256_hex(marker_text.as_bytes()),
    });
}

fn build_protected_source(source: &str, lines: &[&str], regions: &[EditableRegion]) -> String {
    if regions.is_empty() {
        return source.to_string();
    }

    let by_start: BTreeMap<usize, &EditableRegion> = regions
        .iter()
        .map(|region| (region.start_line, region))
        .collect();
    let mut protected = Vec::with_capacity(lines.len() + regions.len());
    let mut line_no = 1usize;

    while line_no <= lines.len() {
        if let Some(region) = by_start.get(&line_no) {
            protected.push(lines[line_no - 1].to_string());
            protected.push(format!(
                "-- REFINEFORGE-INMORTALPROOF-REGION {} {} BODY-SHA256 {}",
                region.id, region.kind, region.body_sha256
            ));
            line_no = region.end_line;
            continue;
        }
        protected.push(lines[line_no - 1].to_string());
        line_no += 1;
    }

    let mut text = protected.join("\n");
    if source.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn initial_sketch(
    source_sha256: &str,
    protected_sha256: &str,
    region_count: usize,
    sorry_count: usize,
    status: &str,
) -> NexusSketch {
    NexusSketch {
        id: "sketch-0000".to_string(),
        parent_id: None,
        source_sha256: source_sha256.to_string(),
        protected_sha256: protected_sha256.to_string(),
        region_count,
        sorry_count,
        validation_status: if status == "ready_for_search" {
            "seed_ready"
        } else {
            "seed_blocked"
        }
        .to_string(),
        elo: 1200.0,
        visits: 0,
        selection: SelectionReceipt {
            algorithm: "p_ucb".to_string(),
            q: 0.0,
            exploration_c: 1.414,
            visit_count: 0,
            total_visits: 0,
            score: 0.0,
        },
    }
}

fn build_episodes(count: usize) -> Vec<NexusEpisode> {
    (0..count)
        .map(|idx| NexusEpisode {
            index: idx + 1,
            parent_sketch_id: "sketch-0000".to_string(),
            mode: "deterministic_receipt_only".to_string(),
            search_replace_calls: 0,
            alphaproof_queries: 0,
            edits_accepted: 0,
            feedback: vec!["initial receipt recorded; no model search executed".to_string()],
            outcome: "recorded".to_string(),
        })
        .collect()
}

fn write_report_artifacts(out_dir: &Path, report: &NexusReport) -> Result<()> {
    write_json(out_dir.join("inmortal-proof-report.json"), report)?;
    std::fs::write(
        out_dir.join("inmortal-proof-report.md"),
        report.to_markdown(),
    )
    .with_context(|| {
        format!(
            "writing {}",
            out_dir.join("inmortal-proof-report.md").display()
        )
    })?;
    write_jsonl(out_dir.join("population.jsonl"), &report.population)?;
    write_jsonl(out_dir.join("episodes.jsonl"), &report.episodes)?;
    write_jsonl(out_dir.join("goal-cache.jsonl"), &report.goal_cache)?;
    Ok(())
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<()> {
    std::fs::write(&path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", path.display()))
}

fn write_jsonl<T: Serialize>(path: PathBuf, values: &[T]) -> Result<()> {
    let mut out = String::new();
    for value in values {
        out.push_str(&serde_json::to_string(value)?);
        out.push('\n');
    }
    std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

impl NexusReport {
    fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Refine-Forge InmortalProof Receipt\n\n");
        out.push_str(&format!("- Status: `{}`\n", self.status));
        out.push_str(&format!("- System: `{}`\n", self.system_name));
        out.push_str(&format!("- Public claim: `{}`\n", self.public_claim));
        out.push_str(&format!("- Claim: `{}`\n", self.claim_id));
        out.push_str(&format!(
            "- Generated at: `{}`\n",
            self.generated_at.to_rfc3339()
        ));
        out.push_str(&format!("- Lean file: `{}`\n", self.lean_file.display()));
        out.push_str(&format!(
            "- Editable regions: `{}`\n",
            self.editable_regions.len()
        ));
        out.push_str(&format!(
            "- Protected source hash: `{}`\n\n",
            self.integrity.protected_sha256
        ));

        out.push_str("## Editable Regions\n\n");
        if self.editable_regions.is_empty() {
            out.push_str("- none\n");
        } else {
            out.push_str("| Region | Kind | Lines | Body SHA-256 |\n");
            out.push_str("|---|---|---:|---|\n");
            for region in &self.editable_regions {
                out.push_str(&format!(
                    "| {} | {} | {}-{} | `{}` |\n",
                    region.id, region.kind, region.start_line, region.end_line, region.body_sha256
                ));
            }
        }

        out.push_str("\n## Blockers\n\n");
        if self.blockers.is_empty() {
            out.push_str("- none\n");
        } else {
            for blocker in &self.blockers {
                out.push_str(&format!("- {blocker}\n"));
            }
        }

        out.push_str("\n## Boundary\n\n");
        out.push_str(&self.boundary);
        out.push('\n');
        out
    }
}

pub fn resolve_out_dir(root: &Path, out_dir: PathBuf) -> Result<PathBuf> {
    if out_dir.as_os_str().is_empty() {
        return Err(anyhow!("Nexus output directory cannot be empty"));
    }
    Ok(if out_dir.is_absolute() {
        out_dir
    } else {
        root.join(out_dir)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_block_and_value_regions() {
        let src = "-- EVOLVE-BLOCK-START\ntrivial\n-- EVOLVE-BLOCK-END\n-- EVOLVE-VALUE-START\ntrue\n-- EVOLVE-VALUE-END\n";
        let scan = scan_evolve_regions(src);

        assert_eq!(scan.regions.len(), 2);
        assert_eq!(scan.regions[0].kind, "block");
        assert_eq!(scan.regions[1].kind, "value");
        assert!(scan.blockers.is_empty());
        assert!(scan
            .protected_source
            .contains("REFINEFORGE-INMORTALPROOF-REGION region-0001 block"));
    }

    #[test]
    fn reports_unterminated_region() {
        let src = "-- EVOLVE-BLOCK-START\ntrivial\n";
        let scan = scan_evolve_regions(src);

        assert_eq!(scan.regions.len(), 0);
        assert!(scan
            .blockers
            .iter()
            .any(|blocker| blocker.contains("unterminated block marker")));
    }
}
