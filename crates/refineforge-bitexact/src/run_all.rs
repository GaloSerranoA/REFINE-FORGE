//! CI-friendly orchestration for running many bit-exact configs.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::experiment::KernelExperiment;
use crate::manifest;
use crate::report::{Outcome, Report};
use crate::runner;

#[derive(Debug, Clone)]
pub struct RunAllOptions {
    pub config_dir: PathBuf,
    pub runs_root: PathBuf,
    pub include_examples: bool,
    pub summary_json: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAllEntry {
    pub config_path: String,
    pub experiment_id: Option<String>,
    pub outcome: Option<Outcome>,
    pub report_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAllSummary {
    pub config_dir: String,
    pub included_configs: usize,
    pub passed: usize,
    pub failed: usize,
    pub entries: Vec<RunAllEntry>,
}

impl RunAllSummary {
    pub fn has_failures(&self) -> bool {
        self.failed > 0
    }
}

pub fn discover_configs(config_dir: &Path, include_examples: bool) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(config_dir)
        .with_context(|| format!("reading config directory {}", config_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !include_examples
            && (file_name.starts_with("example-") || file_name.ends_with("-smoke.yaml"))
        {
            continue;
        }
        paths.push(path);
    }
    paths.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    Ok(paths)
}

pub fn run_directory(options: &RunAllOptions) -> Result<RunAllSummary> {
    let configs = discover_configs(&options.config_dir, options.include_examples)?;
    let mut summary = RunAllSummary {
        config_dir: options.config_dir.display().to_string(),
        included_configs: configs.len(),
        passed: 0,
        failed: 0,
        entries: Vec::with_capacity(configs.len()),
    };

    for config_path in configs {
        let mut entry = RunAllEntry {
            config_path: config_path.display().to_string(),
            experiment_id: None,
            outcome: None,
            report_path: None,
            error: None,
        };

        match run_one(&config_path, &options.runs_root) {
            Ok((experiment_id, outcome, report_path)) => {
                entry.experiment_id = Some(experiment_id);
                entry.outcome = Some(outcome.clone());
                entry.report_path = Some(report_path.display().to_string());
                match outcome {
                    Outcome::Pass => summary.passed += 1,
                    Outcome::Fail => summary.failed += 1,
                }
            }
            Err(err) => {
                summary.failed += 1;
                entry.error = Some(err.to_string());
            }
        }
        summary.entries.push(entry);
    }

    if let Some(path) = &options.summary_json {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&summary)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    Ok(summary)
}

fn run_one(config_path: &Path, runs_root: &Path) -> Result<(String, Outcome, PathBuf)> {
    let exp = KernelExperiment::load(config_path)?;
    let input_manifest = manifest::build_input_manifest(&exp.input_files)?;
    let runs = runner::run_all(runs_root, &exp)?;
    let paths = runner::RunPaths::for_experiment(runs_root, &exp);
    let report = Report::build_with_input_manifest(&exp, runs, input_manifest);
    report.write(&paths.run_dir)?;
    Ok((
        exp.id,
        report.outcome,
        paths.run_dir.join("bitexact-report.json"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &std::path::Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn discover_configs_sorts_and_filters_examples_by_default() {
        let td = tempfile::tempdir().unwrap();
        write(
            &td.path().join("z.yaml"),
            "id: z\ncommand: x\noutput: stdout\n",
        );
        write(
            &td.path().join("example-a.yaml"),
            "id: example-a\ncommand: x\noutput: stdout\n",
        );
        write(
            &td.path().join("helyx-bitexact-smoke.yaml"),
            "id: helyx-bitexact-smoke\ncommand: x\noutput: stdout\n",
        );
        write(
            &td.path().join("a.yaml"),
            "id: a\ncommand: x\noutput: stdout\n",
        );
        write(&td.path().join("notes.txt"), "not yaml");

        let paths = discover_configs(td.path(), false).unwrap();

        let names: Vec<String> = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.yaml", "z.yaml"]);

        let with_examples = discover_configs(td.path(), true).unwrap();
        let with_example_names: Vec<String> = with_examples
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            with_example_names,
            vec![
                "a.yaml",
                "example-a.yaml",
                "helyx-bitexact-smoke.yaml",
                "z.yaml"
            ]
        );
    }

    #[test]
    fn run_directory_records_failures_and_continues() {
        let td = tempfile::tempdir().unwrap();
        let cfg_dir = td.path().join("configs");
        let runs_root = td.path().join("runs");
        std::fs::create_dir(&cfg_dir).unwrap();
        write(
            &cfg_dir.join("a.yaml"),
            r#"
id: fail-a
command: "this-binary-does-not-exist-a"
runs: 2
output: stdout
"#,
        );
        write(
            &cfg_dir.join("b.yaml"),
            r#"
id: fail-b
command: "this-binary-does-not-exist-b"
runs: 2
output: stdout
"#,
        );

        let summary = run_directory(&RunAllOptions {
            config_dir: cfg_dir,
            runs_root,
            include_examples: false,
            summary_json: None,
        })
        .unwrap();

        assert_eq!(summary.entries.len(), 2);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.passed, 0);
        assert!(summary.has_failures());
        assert!(summary
            .entries
            .iter()
            .all(|entry| entry.report_path.is_some()));
    }
}
