use super::common::{
    existing_artifact, AgentMode, AgentReport, AgentStatus, CommandRecord, TrustLevel,
};
use crate::{lint, runner, scan};
use std::path::Path;
use std::time::Instant;

pub fn build(root: &Path, mode: AgentMode, target: &str) -> AgentReport {
    let mut report = AgentReport::new(super::common::AgentKind::Lean, mode, target);
    existing_artifact(root, "docs/verification/proof-inventory.md", &mut report);
    existing_artifact(root, "claims", &mut report);
    existing_artifact(root, "lean/Refineforge.lean", &mut report);

    if !mode.runs_checks() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::ModelOnly,
            "Lean agent inspected proof inventory and claim/Lean surfaces. No implementation correctness is claimed.",
        );
        return report;
    }

    let lean_started = Instant::now();
    let lean = runner::check_all(root);
    report.commands.push(CommandRecord::internal(
        "lean-check-all",
        &["refine", "lean", "check-all"],
        lean_started,
        &lean,
    ));

    let scan_started = Instant::now();
    let scan = scan::scan_all(root);
    report.commands.push(CommandRecord::internal(
        "scan-check-all",
        &["refine", "scan", "check-all"],
        scan_started,
        &scan,
    ));

    let lint_started = Instant::now();
    let lint = lint::lint_all(root);
    report.commands.push(CommandRecord::internal(
        "lint-check-all",
        &["refine", "lint", "check-all"],
        lint_started,
        &lint,
    ));

    if lean.is_ok() && scan.is_ok() && lint.is_ok() {
        report.finish(
            AgentStatus::Passed,
            TrustLevel::ModelLinked,
            "Lean, scan, and claim-lint gates passed. Trust remains bounded by claim scopes and human review fields.",
        );
    } else {
        report.finish(
            AgentStatus::Failed,
            TrustLevel::Blocked,
            "One or more Lean verification gates failed. See command records for the failing gate.",
        );
    }
    report
}
