//! Claim linter: cheap pre-Lean checks for human-maintained claim
//! metadata and refinement-doc drift.

use anyhow::{anyhow, Result};
use std::path::Path;

use crate::claim::{self, Claim};
use crate::scan::{self, ScanStatus};
use serde_yaml::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintIssue {
    pub severity: LintSeverity,
    pub claim_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintReport {
    pub claim_id: String,
    pub issues: Vec<LintIssue>,
}

impl LintReport {
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == LintSeverity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == LintSeverity::Warning)
    }
}

const REQUIRED_REFINEMENT_SECTIONS: &[&str] = &[
    "## 1. What the Lean model says",
    "## 2. What the Rust must implement",
    "## 3. Mapping",
    "## 4. Trusted code base",
    "## 5. What this claim does NOT cover",
    "## 6. Reviewer checklist",
];

pub fn lint_claim(root: &Path, _claim_path: &Path, c: &Claim) -> Result<LintReport> {
    let mut report = LintReport {
        claim_id: c.claim_id.clone(),
        issues: Vec::new(),
    };

    validate_review_metadata(_claim_path, c, &mut report)?;
    validate_scope_disclosure(c, &mut report);

    let scan_report = scan::scan_claim(root, c)?;
    match scan_report.status {
        ScanStatus::FileMissing => {
            for item in &scan_report.items {
                if !item.file_exists {
                    report.error(format!("missing Rust source: {}", item.path));
                }
            }
        }
        ScanStatus::Partial => {
            for item in &scan_report.items {
                for ty in &item.types_missing {
                    report.error(format!(
                        "Rust type `{ty}` is cited but not discovered in {}",
                        item.path
                    ));
                }
                for function in &item.functions_missing {
                    report.error(format!(
                        "Rust function `{function}` is cited but not discovered in {}",
                        item.path
                    ));
                }
            }
        }
        ScanStatus::Verified | ScanStatus::NoRustSource => {}
    }

    let refinement_path = root
        .join("docs")
        .join("refinement")
        .join(format!("{}.md", c.claim_id));
    let expects_refinement = expects_refinement_doc(c);
    if expects_refinement && !refinement_path.exists() {
        report.error(format!(
            "refinement doc missing: {}",
            refinement_path
                .strip_prefix(root)
                .unwrap_or(&refinement_path)
                .display()
        ));
        return Ok(report);
    }

    if refinement_path.exists() {
        let text = std::fs::read_to_string(&refinement_path)?;
        for section in REQUIRED_REFINEMENT_SECTIONS {
            if !text.contains(section) {
                report.warning(format!("missing refinement section: {section}"));
            }
        }
        for src in &c.rust_source {
            for ty in &src.types {
                if !text.contains(ty) {
                    report.warning(format!(
                        "cited Rust type `{ty}` does not appear in refinement doc"
                    ));
                }
            }
            for function in &src.functions {
                if !text.contains(function) {
                    report.warning(format!(
                        "cited Rust function `{function}` does not appear in refinement doc"
                    ));
                }
            }
        }
    }

    Ok(report)
}

pub fn lint_one(root: &Path, claim_id: &str) -> Result<()> {
    let (path, c) = claim::load(root, claim_id)?;
    let report = lint_claim(root, &path, &c)?;
    print_report(&report);
    if report.has_errors() {
        Err(anyhow!("lint of {} failed", claim_id))
    } else {
        Ok(())
    }
}

pub fn lint_all(root: &Path) -> Result<()> {
    let claims = claim::all(root)?;
    if claims.is_empty() {
        println!("(no claims found)");
        return Ok(());
    }
    let mut any_error = false;
    for (path, c) in &claims {
        let report = lint_claim(root, path, c)?;
        let errors = report
            .issues
            .iter()
            .filter(|issue| issue.severity == LintSeverity::Error)
            .count();
        let warnings = report
            .issues
            .iter()
            .filter(|issue| issue.severity == LintSeverity::Warning)
            .count();
        println!(
            "{:<22} errors={} warnings={}",
            report.claim_id, errors, warnings
        );
        if errors > 0 {
            any_error = true;
            for issue in &report.issues {
                if issue.severity == LintSeverity::Error {
                    println!("  ERROR: {}", issue.message);
                }
            }
        }
    }
    if any_error {
        Err(anyhow!("one or more claims failed lint"))
    } else {
        Ok(())
    }
}

impl LintReport {
    fn error(&mut self, message: String) {
        self.issues.push(LintIssue {
            severity: LintSeverity::Error,
            claim_id: self.claim_id.clone(),
            message,
        });
    }

    fn warning(&mut self, message: String) {
        self.issues.push(LintIssue {
            severity: LintSeverity::Warning,
            claim_id: self.claim_id.clone(),
            message,
        });
    }
}

fn expects_refinement_doc(c: &Claim) -> bool {
    let status = c.status.to_ascii_lowercase();
    status.contains("refined") || (status == "proven" && !c.rust_source.is_empty())
}

fn validate_review_metadata(claim_path: &Path, c: &Claim, report: &mut LintReport) -> Result<()> {
    if claim_path.exists() {
        let text = std::fs::read_to_string(claim_path)?;
        let yaml: Value = serde_yaml::from_str(&text)?;
        let Some(review) = yaml_get(&yaml, "review") else {
            report.error(
                "missing review block; use review.human_operator: null until a human signs off"
                    .into(),
            );
            return Ok(());
        };
        match yaml_get(review, "human_operator") {
            None => report.error(
                "missing review.human_operator; use null instead of omitting review status".into(),
            ),
            Some(Value::Null) => {}
            Some(Value::String(operator)) => {
                if is_placeholder_human_operator(operator) {
                    report.error(format!(
                        "review.human_operator uses placeholder/AI identity `{operator}`; keep null until a real human signs"
                    ));
                }
            }
            Some(_) => report
                .error("review.human_operator must be null or a real human identity string".into()),
        }
    }

    if let Some(operator) = &c.review.human_operator {
        if is_placeholder_human_operator(operator) {
            report.error(format!(
                "review.human_operator uses placeholder/AI identity `{operator}`; keep null until a real human signs"
            ));
        }
    }

    Ok(())
}

fn validate_scope_disclosure(c: &Claim, report: &mut LintReport) {
    if !c.claim_id.starts_with("CLAIM-CRS-") {
        return;
    }

    let scope = c.scope.trim().to_ascii_lowercase();
    if scope != "model-only" && scope != "model_only" {
        report.error("CRS claims must stay scope: model-only until a human-reviewed implementation refinement exists".into());
    }

    let description = c.description.to_ascii_lowercase();
    let notes = c.review.notes.to_ascii_lowercase();
    if !description.contains("honest scope")
        || !(description.contains("model") || notes.contains("model"))
    {
        report.error("CRS model-only claims must disclose the model-only limitation in description/review notes with `Honest scope`".into());
    }

    if !c.rust_source.is_empty() {
        report.error(
            "CRS model-only claims may not cite Rust implementation sources without upgrading scope and review".into(),
        );
    }
}

fn yaml_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Mapping(map) = value else {
        return None;
    };
    map.get(Value::String(key.to_string()))
}

fn is_placeholder_human_operator(operator: &str) -> bool {
    let normalized = operator.trim().to_ascii_lowercase();
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "ai" | "assistant"
                | "bot"
                | "claude"
                | "codex"
                | "fake"
                | "llm"
                | "none"
                | "null"
                | "placeholder"
                | "tbd"
                | "todo"
                | "unknown"
        )
}

fn print_report(report: &LintReport) {
    println!("claim: {}", report.claim_id);
    if report.issues.is_empty() {
        println!("status: OK");
        return;
    }
    for issue in &report.issues {
        let label = match issue.severity {
            LintSeverity::Error => "ERROR",
            LintSeverity::Warning => "WARNING",
        };
        println!("{label}: {}", issue.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::{Claim, LeanInfo, Policy, ReviewInfo, RustSource};
    use std::path::Path;

    fn base_claim(id: &str, status: &str, rust_path: &str) -> Claim {
        Claim {
            claim_id: id.into(),
            title: "lint test".into(),
            description: String::new(),
            scope: "test".into(),
            status: status.into(),
            authors: vec!["test".into()],
            rust_source: vec![RustSource {
                path: rust_path.into(),
                types: vec!["Counter".into()],
                functions: vec!["incr".into()],
            }],
            lean: LeanInfo {
                toolchain: "leanprover/lean4:v4.29.1".into(),
                module: "Refineforge.Test".into(),
                file: "lean/Refineforge/Test.lean".into(),
                theorems: vec!["test_theorem".into()],
            },
            policy: Policy::default(),
            review: ReviewInfo::default(),
        }
    }

    fn write_rust(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            "pub struct Counter { value: u64 }\npub fn incr(c: &Counter) -> Counter { Counter { value: c.value + 1 } }\n",
        )
        .unwrap();
    }

    fn write_refinement(root: &Path, claim_id: &str, body: &str) {
        let dir = root.join("docs").join("refinement");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{claim_id}.md")), body).unwrap();
    }

    fn write_claim_yaml(root: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let dir = root.join("claims");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    fn complete_refinement_doc() -> &'static str {
        "# TEST\n\n\
## 1. What the Lean model says\nCounter theorem.\n\n\
## 2. What the Rust must implement\nCounter incr.\n\n\
## 3. Mapping\nCounter maps to Counter and incr maps to incr.\n\n\
## 4. Trusted code base\nLean and Rust.\n\n\
## 5. What this claim does NOT cover\nConcurrency.\n\n\
## 6. Reviewer checklist\n- [x] scan\n"
    }

    #[test]
    fn lint_flags_missing_rust_source_file() {
        let td = tempfile::tempdir().unwrap();
        let claim = base_claim("TEST-LINT-001", "drafted", "src/missing.rs");

        let report = lint_claim(td.path(), &td.path().join("claims/test.yaml"), &claim).unwrap();

        assert!(report.has_errors());
        assert!(report
            .issues
            .iter()
            .any(|i| i.message.contains("missing Rust source")));
    }

    #[test]
    fn lint_flags_refined_claim_without_refinement_doc() {
        let td = tempfile::tempdir().unwrap();
        write_rust(td.path(), "src/lib.rs");
        let claim = base_claim("TEST-LINT-002", "model+refined", "src/lib.rs");

        let report = lint_claim(td.path(), &td.path().join("claims/test.yaml"), &claim).unwrap();

        assert!(report.has_errors());
        assert!(report
            .issues
            .iter()
            .any(|i| i.message.contains("refinement doc missing")));
    }

    #[test]
    fn lint_flags_refinement_doc_missing_required_sections() {
        let td = tempfile::tempdir().unwrap();
        write_rust(td.path(), "src/lib.rs");
        write_refinement(td.path(), "TEST-LINT-003", "# incomplete\n\nCounter incr\n");
        let claim = base_claim("TEST-LINT-003", "model+refined", "src/lib.rs");

        let report = lint_claim(td.path(), &td.path().join("claims/test.yaml"), &claim).unwrap();

        assert!(report.has_warnings());
        assert!(report
            .issues
            .iter()
            .any(|i| i.message.contains("missing refinement section")));
    }

    #[test]
    fn lint_passes_example_counter_shape() {
        let td = tempfile::tempdir().unwrap();
        write_rust(td.path(), "src/lib.rs");
        write_refinement(td.path(), "TEST-LINT-004", complete_refinement_doc());
        let claim = base_claim("TEST-LINT-004", "model+refined", "src/lib.rs");

        let report = lint_claim(td.path(), &td.path().join("claims/test.yaml"), &claim).unwrap();

        assert!(!report.has_errors(), "{:?}", report.issues);
    }

    #[test]
    fn lint_flags_missing_explicit_human_operator_field() {
        let td = tempfile::tempdir().unwrap();
        let path = write_claim_yaml(
            td.path(),
            "missing-review.yaml",
            "claim_id: TEST-LINT-005\nreview:\n  reviewed_on: null\n",
        );
        let claim = base_claim("TEST-LINT-005", "proven", "src/none.rs");

        let report = lint_claim(td.path(), &path, &claim).unwrap();

        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.message.contains("review.human_operator")),
            "{report:?}"
        );
        assert!(report.has_errors());
    }

    #[test]
    fn lint_rejects_ai_placeholder_human_operator() {
        let td = tempfile::tempdir().unwrap();
        let path = write_claim_yaml(
            td.path(),
            "fake-reviewer.yaml",
            "claim_id: TEST-LINT-006\nreview:\n  human_operator: codex\n  reviewed_on: null\n",
        );
        let claim = base_claim("TEST-LINT-006", "proven", "src/none.rs");

        let report = lint_claim(td.path(), &path, &claim).unwrap();

        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.message.contains("placeholder")),
            "{report:?}"
        );
        assert!(report.has_errors());
    }

    #[test]
    fn lint_rejects_crs_claim_without_model_only_scope() {
        let td = tempfile::tempdir().unwrap();
        let path = write_claim_yaml(
            td.path(),
            "CLAIM-CRS-999.yaml",
            "claim_id: CLAIM-CRS-999\nreview:\n  human_operator: null\n",
        );
        let mut claim = base_claim("CLAIM-CRS-999", "proven", "src/none.rs");
        claim.scope = "model+refined".into();
        claim.description = "No disclosure.".into();

        let report = lint_claim(td.path(), &path, &claim).unwrap();

        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.message.contains("model-only")),
            "{report:?}"
        );
        assert!(report.has_errors());
    }
}
