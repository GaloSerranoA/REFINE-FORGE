//! Experiment configuration: the YAML an ML engineer writes to
//! define one training run.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Stable identifier; used as the run directory name.
    /// Conventionally `YYYY-MM-DD-shortname-vN`.
    pub id: String,

    #[serde(default)]
    pub description: String,

    pub base_model: BaseModel,
    pub dataset: Dataset,
    pub backend: Backend,

    #[serde(default)]
    pub hyperparameters: BTreeMap<String, serde_yaml::Value>,

    #[serde(default)]
    pub checkpoint: CheckpointConfig,

    #[serde(default)]
    pub monitoring: MonitoringConfig,

    #[serde(default)]
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseModel {
    /// e.g. "Qwen/Qwen2.5-Coder-1.5B"
    pub name: String,
    /// Where the base weights come from: `huggingface`, `local`,
    /// `s3`, etc. The runner doesn't enforce specific values; the
    /// backend script reads this and acts accordingly.
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub revision: Option<String>,
}

fn default_source() -> String {
    "huggingface".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub path: PathBuf,
    /// e.g. `jsonl`, `parquet`, `arrow`. Passed through to backend.
    #[serde(default = "default_format")]
    pub format: String,
    /// Optional field mapping for backends that need it.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

fn default_format() -> String {
    "jsonl".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    /// One of: `refineforge_native`, `axolotl`, `hf_trainer`,
    /// `helyx_train`, `custom`. Runner uses this to pick the right
    /// in-process backend or subprocess pattern + log parser.
    pub kind: String,
    /// Path to the backend's own config file (e.g. axolotl YAML).
    #[serde(default)]
    pub config_file: Option<PathBuf>,
    /// Shell command template. Tokens `{run_dir}`, `{config_file}`,
    /// `{dataset_path}`, `{resume_from}` are substituted at run time.
    /// If omitted, the runner uses a backend-default template.
    #[serde(default)]
    pub command: Option<String>,
    /// Extra args passed verbatim AFTER the substituted command.
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Backend-specific runtime metadata. HRM-Text uses this for
    /// `source_repo`, `python`, `torchrun`, `nproc_per_node`, and cluster
    /// launch hints without forcing those fields onto every backend.
    #[serde(default)]
    pub runtime: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Where the backend writes checkpoints, relative to run_dir.
    /// Default: `checkpoints/`.
    #[serde(default = "default_checkpoint_dir")]
    pub dir: String,
    /// How often the backend should save (in steps). Informational
    /// for the runner; the backend has its own setting too.
    #[serde(default)]
    pub save_steps: Option<u64>,
    /// Keep at most N most-recent checkpoints. The runner prunes
    /// older ones AFTER each successful step boundary.
    #[serde(default)]
    pub keep_last: Option<usize>,
}

fn default_checkpoint_dir() -> String {
    "checkpoints".to_string()
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            dir: default_checkpoint_dir(),
            save_steps: None,
            keep_last: Some(3),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Where stdout/stderr is captured, relative to run_dir.
    #[serde(default = "default_log_file")]
    pub log_file: String,
    /// `huggingface`, `axolotl`, `generic`. Picks the progress parser.
    #[serde(default = "default_progress_format")]
    pub progress_format: String,
    /// Metric names to surface in the report. Other metrics are still
    /// captured to `progress.jsonl` but not summarised.
    #[serde(default = "default_metrics")]
    pub metrics_to_track: Vec<String>,
}

fn default_log_file() -> String {
    "train.log".to_string()
}
fn default_progress_format() -> String {
    "generic".to_string()
}
fn default_metrics() -> Vec<String> {
    vec!["loss".into(), "eval_loss".into(), "learning_rate".into()]
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            log_file: default_log_file(),
            progress_format: default_progress_format(),
            metrics_to_track: default_metrics(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_backoff_seconds")]
    pub backoff_seconds: u64,
    #[serde(default = "default_true")]
    pub resume_from_checkpoint: bool,
}

fn default_max_attempts() -> u32 {
    3
}
fn default_backoff_seconds() -> u64 {
    60
}
fn default_true() -> bool {
    true
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            backoff_seconds: default_backoff_seconds(),
            resume_from_checkpoint: true,
        }
    }
}

impl Experiment {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let exp: Experiment = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing experiment YAML {}", path.display()))?;
        exp.validate()?;
        Ok(exp)
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("experiment.id may not be empty");
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            anyhow::bail!(
                "experiment.id must be alphanumeric + `-_.` (got {:?})",
                self.id
            );
        }
        match self.backend.kind.as_str() {
            "refineforge_native"
            | "refineforge_native_causal_lm"
            | "refineforge_native_gpt"
            | "refineforge_native_gpt_cuda"
            | "axolotl"
            | "hf_trainer"
            | "helyx_train"
            | "hrm_text"
            | "pytorch_baseline"
            | "custom" => {}
            other => anyhow::bail!(
                "unknown backend.kind {:?} — supported: refineforge_native, refineforge_native_causal_lm, refineforge_native_gpt, refineforge_native_gpt_cuda, axolotl, hf_trainer, helyx_train, hrm_text, pytorch_baseline, custom",
                other
            ),
        }
        if matches!(
            self.backend.kind.as_str(),
            "refineforge_native"
                | "refineforge_native_causal_lm"
                | "refineforge_native_gpt"
                | "refineforge_native_gpt_cuda"
        ) && self.backend.command.is_some()
        {
            anyhow::bail!(
                "backend.kind={} does not accept backend.command",
                self.backend.kind
            );
        }
        if matches!(self.backend.kind.as_str(), "custom" | "pytorch_baseline")
            && self.backend.command.is_none()
        {
            anyhow::bail!(
                "backend.kind={} requires backend.command template",
                self.backend.kind
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(yaml: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exp.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        (dir, path)
    }

    const MINIMAL: &str = r#"
id: 2026-05-18-test-v1
description: "minimal experiment"
base_model:
  name: "Qwen/Qwen2.5-Coder-1.5B"
dataset:
  path: training/data/foo.jsonl
backend:
  kind: hf_trainer
hyperparameters:
  learning_rate: 0.0002
  batch_size: 8
"#;

    #[test]
    fn loads_minimal_experiment() {
        let (_dir, path) = write_temp(MINIMAL);
        let exp = Experiment::load(&path).unwrap();
        assert_eq!(exp.id, "2026-05-18-test-v1");
        assert_eq!(exp.base_model.name, "Qwen/Qwen2.5-Coder-1.5B");
        assert_eq!(exp.base_model.source, "huggingface");
        assert_eq!(exp.backend.kind, "hf_trainer");
        // Defaults applied:
        assert_eq!(exp.checkpoint.keep_last, Some(3));
        assert_eq!(exp.monitoring.log_file, "train.log");
        assert_eq!(exp.retry.max_attempts, 3);
    }

    #[test]
    fn rejects_empty_id() {
        let yaml = MINIMAL.replace("2026-05-18-test-v1", "");
        let (_dir, path) = write_temp(&yaml);
        let err = Experiment::load(&path).unwrap_err();
        assert!(err.to_string().contains("id may not be empty"), "{err}");
    }

    #[test]
    fn rejects_bad_id_chars() {
        let yaml = MINIMAL.replace("2026-05-18-test-v1", "with spaces");
        let (_dir, path) = write_temp(&yaml);
        let err = Experiment::load(&path).unwrap_err();
        assert!(err.to_string().contains("alphanumeric"), "{err}");
    }

    #[test]
    fn rejects_unknown_backend() {
        let yaml = MINIMAL.replace("hf_trainer", "magic-trainer");
        let (_dir, path) = write_temp(&yaml);
        let err = Experiment::load(&path).unwrap_err();
        assert!(err.to_string().contains("unknown backend.kind"), "{err}");
    }

    #[test]
    fn custom_backend_requires_command() {
        let yaml = MINIMAL.replace("hf_trainer", "custom");
        let (_dir, path) = write_temp(&yaml);
        let err = Experiment::load(&path).unwrap_err();
        assert!(err.to_string().contains("command template"), "{err}");
    }

    #[test]
    fn loads_helyx_train_backend() {
        let yaml = MINIMAL.replace(
            "kind: hf_trainer",
            "kind: helyx_train\n  config_file: training/configs/helyx-proof-repair.yaml",
        );
        let (_dir, path) = write_temp(&yaml);
        let exp = Experiment::load(&path).unwrap();
        assert_eq!(exp.backend.kind, "helyx_train");
        assert_eq!(
            exp.backend.config_file.as_deref(),
            Some(Path::new("training/configs/helyx-proof-repair.yaml"))
        );
    }

    #[test]
    fn loads_refineforge_native_backend() {
        let yaml = MINIMAL.replace("kind: hf_trainer", "kind: refineforge_native");
        let (_dir, path) = write_temp(&yaml);
        let exp = Experiment::load(&path).unwrap();
        assert_eq!(exp.backend.kind, "refineforge_native");
    }

    #[test]
    fn native_backend_rejects_subprocess_command() {
        let yaml = MINIMAL.replace(
            "kind: hf_trainer",
            "kind: refineforge_native\n  command: echo should-not-run",
        );
        let (_dir, path) = write_temp(&yaml);
        let err = Experiment::load(&path).unwrap_err();
        assert!(
            err.to_string().contains("does not accept backend.command"),
            "{err}"
        );
    }
}
