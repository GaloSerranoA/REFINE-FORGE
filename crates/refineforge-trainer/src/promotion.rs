use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::report::{CheckpointInfo, Report};

const LOCAL_FINETUNE_MANIFEST: &str = "refineforge-local-finetune.json";
const PROMOTION_REPORT: &str = "promotion-report.json";

#[derive(Debug, Clone)]
pub struct PromotionOptions {
    pub run_dir: PathBuf,
    pub out_dir: PathBuf,
    pub model_id: String,
    pub command: Vec<String>,
    pub producer: String,
    pub require_success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionReport {
    pub status: String,
    pub blockers: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub run_dir: String,
    pub source_report: String,
    pub out_dir: String,
    pub manifest_path: Option<String>,
    pub model_id: String,
    pub producer: String,
    pub source_experiment_id: String,
    pub backend_kind: String,
    pub base_model: String,
    pub dataset_path: String,
    pub checkpoint: Option<PromotedCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotedCheckpoint {
    pub step: u64,
    pub path: String,
    pub size_bytes: u64,
}

pub fn promote(opts: &PromotionOptions) -> Result<PromotionReport> {
    let source_report = opts.run_dir.join("report.json");
    let text = std::fs::read_to_string(&source_report)
        .with_context(|| format!("reading {}", source_report.display()))?;
    let training_report: Report = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", source_report.display()))?;

    let mut blockers = Vec::new();
    if opts.model_id.trim().is_empty() {
        blockers.push("model_id may not be empty".to_string());
    }
    if opts.command.is_empty() {
        blockers.push("local-finetune runtime command may not be empty".to_string());
    }
    if opts.require_success && training_report.final_outcome != "success" {
        blockers.push(format!(
            "training report final_outcome must be success, got {:?}",
            training_report.final_outcome
        ));
    }

    let latest = latest_checkpoint(&training_report.checkpoints);
    let checkpoint = latest.map(|checkpoint| {
        let resolved_path = resolve_checkpoint_path(&opts.run_dir, &checkpoint.path);
        if !resolved_path.exists() {
            blockers.push(format!(
                "checkpoint path does not exist: {}",
                resolved_path.display()
            ));
        }
        PromotedCheckpoint {
            step: checkpoint.step,
            path: resolved_path.display().to_string(),
            size_bytes: checkpoint.size_bytes,
        }
    });
    if latest.is_none() {
        blockers.push("training report has no checkpoints".to_string());
    }

    std::fs::create_dir_all(&opts.out_dir)
        .with_context(|| format!("creating {}", opts.out_dir.display()))?;

    let manifest_path = opts.out_dir.join(LOCAL_FINETUNE_MANIFEST);
    let status = if blockers.is_empty() { "ready" } else { "blocked" };
    let promotion_report = PromotionReport {
        status: status.to_string(),
        blockers,
        created_at: Utc::now(),
        run_dir: opts.run_dir.display().to_string(),
        source_report: source_report.display().to_string(),
        out_dir: opts.out_dir.display().to_string(),
        manifest_path: (status == "ready").then(|| manifest_path.display().to_string()),
        model_id: opts.model_id.clone(),
        producer: opts.producer.clone(),
        source_experiment_id: training_report.experiment.id.clone(),
        backend_kind: training_report.experiment.backend.kind.clone(),
        base_model: training_report.experiment.base_model.name.clone(),
        dataset_path: training_report.experiment.dataset.path.display().to_string(),
        checkpoint: checkpoint.clone(),
    };

    if status == "ready" {
        let checkpoint = checkpoint.expect("ready promotion has a checkpoint");
        let command = opts
            .command
            .iter()
            .map(|arg| substitute_manifest_token(arg, opts, &checkpoint.path))
            .collect::<Vec<_>>();
        let manifest = json!({
            "runtime": "command",
            "model_id": opts.model_id,
            "command": command,
            "producer": {
                "kind": opts.producer,
                "source_experiment_id": training_report.experiment.id,
                "backend_kind": training_report.experiment.backend.kind,
                "base_model": training_report.experiment.base_model.name,
                "source_report": source_report.display().to_string()
            },
            "checkpoint": {
                "step": checkpoint.step,
                "path": checkpoint.path,
                "size_bytes": checkpoint.size_bytes
            },
            "dataset": {
                "path": training_report.experiment.dataset.path.display().to_string(),
                "format": training_report.experiment.dataset.format
            }
        });
        std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
            .with_context(|| format!("writing {}", manifest_path.display()))?;
    }

    let promotion_report_path = opts.out_dir.join(PROMOTION_REPORT);
    std::fs::write(
        &promotion_report_path,
        serde_json::to_string_pretty(&promotion_report)?,
    )
    .with_context(|| format!("writing {}", promotion_report_path.display()))?;

    if promotion_report.status != "ready" {
        return Err(anyhow!(
            "promotion blocked: {}",
            promotion_report.blockers.join("; ")
        ));
    }
    Ok(promotion_report)
}

fn latest_checkpoint(checkpoints: &[CheckpointInfo]) -> Option<&CheckpointInfo> {
    checkpoints.iter().max_by_key(|checkpoint| checkpoint.step)
}

fn resolve_checkpoint_path(run_dir: &Path, checkpoint_path: &str) -> PathBuf {
    let raw = PathBuf::from(checkpoint_path);
    if raw.is_absolute() || raw.exists() {
        return raw;
    }
    let from_run_dir = run_dir.join(&raw);
    if from_run_dir.exists() {
        return from_run_dir;
    }
    raw
}

fn substitute_manifest_token(arg: &str, opts: &PromotionOptions, checkpoint_path: &str) -> String {
    arg.replace("{checkpoint_dir}", checkpoint_path)
        .replace("{checkpoint_path}", checkpoint_path)
        .replace("{run_dir}", &opts.run_dir.display().to_string())
        .replace("{out_dir}", &opts.out_dir.display().to_string())
        .replace("{model_id}", &opts.model_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{
        Backend, BaseModel, CheckpointConfig, Dataset, Experiment, MonitoringConfig, RetryConfig,
    };
    use crate::report::{CheckpointInfo, Report};
    use chrono::Utc;
    use std::collections::BTreeMap;

    fn report(final_outcome: &str, checkpoint: Option<(&std::path::Path, u64)>) -> Report {
        Report {
            experiment: Experiment {
                id: "promotion-fixture".into(),
                description: "promotion fixture".into(),
                base_model: BaseModel {
                    name: "fixture-base".into(),
                    source: "local".into(),
                    revision: None,
                },
                dataset: Dataset {
                    path: "training/data/fixture.jsonl".into(),
                    format: "jsonl".into(),
                    fields: BTreeMap::new(),
                },
                backend: Backend {
                    kind: "helyx_train".into(),
                    config_file: Some("training/configs/fixture-helyx.yaml".into()),
                    command: None,
                    extra_args: vec![],
                },
                hyperparameters: BTreeMap::new(),
                checkpoint: CheckpointConfig::default(),
                monitoring: MonitoringConfig::default(),
                retry: RetryConfig::default(),
            },
            finished_at: Utc::now(),
            final_outcome: final_outcome.into(),
            attempts: 1,
            progress_record_count: 1,
            metric_summary: BTreeMap::new(),
            checkpoints: checkpoint
                .map(|(path, step)| {
                    vec![CheckpointInfo {
                        step,
                        path: path.display().to_string(),
                        size_bytes: 12,
                    }]
                })
                .unwrap_or_default(),
            failures: vec![],
        }
    }

    fn write_report(run_dir: &std::path::Path, report: &Report) {
        std::fs::create_dir_all(run_dir).unwrap();
        std::fs::write(
            run_dir.join("report.json"),
            serde_json::to_string_pretty(report).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn promotion_writes_local_finetune_manifest_for_successful_checkpoint() {
        let td = tempfile::tempdir().unwrap();
        let run_dir = td.path().join("run");
        let checkpoint_dir = run_dir.join("checkpoints").join("step-5");
        std::fs::create_dir_all(&checkpoint_dir).unwrap();
        write_report(&run_dir, &report("success", Some((&checkpoint_dir, 5))));
        let out_dir = td.path().join("promoted");

        let promotion = promote(&PromotionOptions {
            run_dir: run_dir.clone(),
            out_dir: out_dir.clone(),
            model_id: "helyx-proof-repair-smoke".into(),
            command: vec![
                "helyx-infer".into(),
                "--checkpoint".into(),
                "{checkpoint_dir}".into(),
            ],
            producer: "helyx-train".into(),
            require_success: true,
        })
        .unwrap();

        assert_eq!(promotion.status, "ready");
        assert!(out_dir.join("refineforge-local-finetune.json").exists());
        assert!(out_dir.join("promotion-report.json").exists());

        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out_dir.join("refineforge-local-finetune.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["runtime"], "command");
        assert_eq!(manifest["model_id"], "helyx-proof-repair-smoke");
        assert_eq!(manifest["producer"]["kind"], "helyx-train");
        assert_eq!(manifest["checkpoint"]["step"], 5);
        assert!(
            manifest["command"][2]
                .as_str()
                .unwrap()
                .contains("step-5")
        );
    }

    #[test]
    fn promotion_blocks_failed_training_report() {
        let td = tempfile::tempdir().unwrap();
        let run_dir = td.path().join("run");
        let checkpoint_dir = run_dir.join("checkpoints").join("step-5");
        std::fs::create_dir_all(&checkpoint_dir).unwrap();
        write_report(&run_dir, &report("failure", Some((&checkpoint_dir, 5))));
        let out_dir = td.path().join("promoted");

        let err = promote(&PromotionOptions {
            run_dir,
            out_dir: out_dir.clone(),
            model_id: "blocked".into(),
            command: vec!["runtime".into()],
            producer: "helyx-train".into(),
            require_success: true,
        })
        .unwrap_err();

        assert!(err.to_string().contains("promotion blocked"), "{err}");
        assert!(out_dir.join("promotion-report.json").exists());
        assert!(!out_dir.join("refineforge-local-finetune.json").exists());
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out_dir.join("promotion-report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["status"], "blocked");
        assert!(report["blockers"][0].as_str().unwrap().contains("success"));
    }
}
