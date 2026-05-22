pub mod common;
mod devops;
mod kernel;
mod lean;
mod train;

use anyhow::{bail, Result};
use common::{write_reports, AgentKind, AgentReport, AgentStatus, TrustLevel};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AgentOptions {
    pub mode: AgentMode,
    pub target: String,
    pub out_dir: PathBuf,
    pub emit_json: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum AgentRole {
    Lean,
    Devops,
    Train,
    Kernel,
}

impl AgentRole {
    fn kind(self) -> AgentKind {
        match self {
            AgentRole::Lean => AgentKind::Lean,
            AgentRole::Devops => AgentKind::Devops,
            AgentRole::Train => AgentKind::Train,
            AgentRole::Kernel => AgentKind::Kernel,
        }
    }
}

pub use common::AgentMode;

pub fn run_role(root: &Path, role: AgentRole, opts: AgentOptions) -> Result<()> {
    let report = build_role_report(root, role, &opts);
    write_and_print(
        &opts.out_dir,
        role.kind().report_stem(),
        &report,
        opts.emit_json,
    )?;
    status_to_result(&report)
}

pub fn run_all(root: &Path, opts: AgentOptions) -> Result<()> {
    std::fs::create_dir_all(&opts.out_dir)?;
    let mut reports = Vec::new();
    for role in [
        AgentRole::Lean,
        AgentRole::Devops,
        AgentRole::Train,
        AgentRole::Kernel,
    ] {
        let report = build_role_report(root, role, &opts);
        write_reports(&opts.out_dir, role.kind().report_stem(), &report)?;
        reports.push(report);
    }

    let mut summary = AgentReport::new(AgentKind::RunAll, opts.mode, opts.target.clone());
    summary.artifacts.extend(
        ["lean.json", "devops.json", "train.json", "kernel.json"]
            .into_iter()
            .map(PathBuf::from),
    );
    for report in &reports {
        if report.status != AgentStatus::Passed {
            summary.blockers.push(format!(
                "{} agent ended with status {}",
                report.agent.as_str(),
                report.status.as_str()
            ));
        }
        for warning in &report.warnings {
            summary
                .warnings
                .push(format!("{}: {warning}", report.agent.as_str()));
        }
    }
    let status = if summary.blockers.is_empty() {
        AgentStatus::Passed
    } else if reports.iter().any(|r| r.status == AgentStatus::Passed) {
        AgentStatus::Partial
    } else {
        AgentStatus::Failed
    };
    let trust_level = if status == AgentStatus::Passed {
        lowest_trust(&reports)
    } else {
        TrustLevel::Blocked
    };
    summary.finish(
        status,
        trust_level,
        "Combined HELYX readiness dashboard generated from the four role reports.",
    );
    write_and_print(
        &opts.out_dir,
        AgentKind::RunAll.report_stem(),
        &summary,
        opts.emit_json,
    )?;
    status_to_result(&summary)
}

fn build_role_report(root: &Path, role: AgentRole, opts: &AgentOptions) -> AgentReport {
    match role {
        AgentRole::Lean => lean::build(root, opts.mode, &opts.target),
        AgentRole::Devops => devops::build(root, opts.mode, &opts.target, &opts.out_dir),
        AgentRole::Train => train::build(root, opts.mode, &opts.target, &opts.out_dir),
        AgentRole::Kernel => kernel::build(root, opts.mode, &opts.target, &opts.out_dir),
    }
}

fn write_and_print(
    out_dir: &Path,
    stem: &str,
    report: &AgentReport,
    emit_json: bool,
) -> Result<()> {
    write_reports(out_dir, stem, report)?;
    if emit_json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "agent report: {} ({})",
            out_dir.join(format!("{stem}.md")).display(),
            report.status.as_str()
        );
    }
    Ok(())
}

fn status_to_result(report: &AgentReport) -> Result<()> {
    if report.status.is_success() {
        Ok(())
    } else {
        bail!(
            "{} agent finished with status {}",
            report.agent.as_str(),
            report.status.as_str()
        )
    }
}

fn lowest_trust(reports: &[AgentReport]) -> TrustLevel {
    if reports.iter().any(|r| r.trust_level == TrustLevel::Blocked) {
        TrustLevel::Blocked
    } else if reports
        .iter()
        .any(|r| r.trust_level == TrustLevel::MeasuredOnly)
    {
        TrustLevel::MeasuredOnly
    } else if reports
        .iter()
        .any(|r| r.trust_level == TrustLevel::ModelOnly)
    {
        TrustLevel::ModelOnly
    } else if reports
        .iter()
        .any(|r| r.trust_level == TrustLevel::ModelLinked)
    {
        TrustLevel::ModelLinked
    } else if reports
        .iter()
        .any(|r| r.trust_level == TrustLevel::ReleaseReadyLocal)
    {
        TrustLevel::ReleaseReadyLocal
    } else if reports
        .iter()
        .any(|r| r.trust_level == TrustLevel::ReleaseReadyCi)
    {
        TrustLevel::ReleaseReadyCi
    } else {
        TrustLevel::HumanReviewed
    }
}
