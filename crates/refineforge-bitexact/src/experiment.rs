//! Kernel-test experiment configuration: the YAML a CUDA engineer
//! writes to define one bit-exact gate check.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelExperiment {
    /// Stable identifier; used as the run directory name.
    pub id: String,

    /// Optional config template/schema marker for future compatibility.
    #[serde(default)]
    pub template_version: Option<String>,

    #[serde(default)]
    pub description: String,

    /// External producer of the kernel implementation, e.g. `helyx-kernels`.
    #[serde(default)]
    pub producer: Option<String>,

    /// Stable producer-side kernel id, e.g. `helyx.attention.rope_v1`.
    #[serde(default)]
    pub kernel_id: Option<String>,

    /// Readiness/audit profile. Strict profile checks are enforced by linting.
    #[serde(default)]
    pub profile: KernelProfile,

    /// Shell command to invoke the kernel. Tokens `{run_dir}`,
    /// `{run_index}` are substituted per-run. If the command writes
    /// its output to stdout, set `output: stdout`. If it writes to a
    /// file, set `output: { file: <path> }` where `<path>` may also
    /// contain `{run_dir}` and `{run_index}`.
    pub command: String,

    /// How many times to run the kernel. Must be ≥ 2 for the gate
    /// to have any meaning. Recommended: 5+ for robustness.
    #[serde(default = "default_runs")]
    pub runs: usize,

    /// Where the kernel's deterministic output lives.
    pub output: OutputSource,

    /// Optional known-good output hash. If present, all runs must match it.
    #[serde(default)]
    pub expected_sha256: Option<String>,

    /// Deterministic input files whose hashes are recorded in the report.
    #[serde(default)]
    pub input_files: Vec<PathBuf>,

    /// Search/filter metadata. Sorted on load for deterministic reports.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Environment variables set on every run. The most important
    /// ones for CUDA determinism (see docs/bit-exact-reproducibility.md):
    ///   - CUBLAS_WORKSPACE_CONFIG=:4096:8
    ///   - CUDA_LAUNCH_BLOCKING=1
    ///   - PYTHONHASHSEED=0
    ///   - TF_DETERMINISTIC_OPS=1
    /// The runner does NOT inject these automatically — the
    /// engineer's config is the source of truth. We make them
    /// trivial to enumerate.
    #[serde(default)]
    pub env: BTreeMap<String, String>,

    /// Optional: extra hardware metadata to record in the report
    /// (GPU model, CUDA version, driver version, hostname). The
    /// runner does NOT auto-detect these; the engineer fills them
    /// in or the CI job populates them from `nvidia-smi`.
    #[serde(default)]
    pub hardware: BTreeMap<String, String>,
}

fn default_runs() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KernelProfile {
    Generic,
    CudaStrict,
    HelyxCuda,
}

impl Default for KernelProfile {
    fn default() -> Self {
        Self::Generic
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputSource {
    /// Hash the bytes written to stdout. YAML: `output: stdout`.
    Stdout,
    /// Hash the bytes of a file written by the kernel. The string
    /// may contain `{run_dir}` and `{run_index}` tokens.
    /// YAML: `output: {file: "..."}` or
    /// ```yaml
    /// output:
    ///   file: "..."
    /// ```
    File(String),
}

// Custom Deserialize so we accept the user-friendly forms
// `output: stdout` (bare string) and `output: {file: "<path>"}`
// (map) without requiring YAML !tag syntax.
impl<'de> Deserialize<'de> for OutputSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = OutputSource;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("'stdout' (bare string) or {file: '<path>'} (map)")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v {
                    "stdout" => Ok(OutputSource::Stdout),
                    other => Err(E::custom(format!(
                        "unknown output kind {other:?}; expected 'stdout' or {{file: <path>}}"
                    ))),
                }
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
                let mut file: Option<String> = None;
                while let Some(key) = m.next_key::<String>()? {
                    match key.as_str() {
                        "file" => file = Some(m.next_value()?),
                        other => {
                            return Err(de::Error::custom(format!(
                                "unknown output key {other:?}; expected 'file'"
                            )));
                        }
                    }
                }
                file.map(OutputSource::File)
                    .ok_or_else(|| de::Error::custom("expected 'file' key in output map"))
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl KernelExperiment {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut exp: KernelExperiment = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing kernel-experiment YAML {}", path.display()))?;
        exp.input_files.sort();
        exp.tags.sort();
        exp.tags.dedup();
        exp.validate()?;
        Ok(exp)
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("kernel_experiment.id may not be empty");
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            anyhow::bail!(
                "kernel_experiment.id must be alphanumeric + `-_.` (got {:?})",
                self.id
            );
        }
        if self.runs < 2 {
            anyhow::bail!(
                "kernel_experiment.runs must be ≥ 2 (got {}) — a single run cannot prove bit-exactness",
                self.runs
            );
        }
        if self.command.trim().is_empty() {
            anyhow::bail!("kernel_experiment.command may not be empty");
        }
        if let Some(expected) = &self.expected_sha256 {
            if !is_lower_sha256(expected) {
                anyhow::bail!(
                    "kernel_experiment.expected_sha256 must be a 64-character lowercase SHA-256 hex string"
                );
            }
        }
        Ok(())
    }

    /// Resolve `{run_dir}` and `{run_index}` tokens in a string.
    pub fn substitute(&self, template: &str, run_dir: &Path, run_index: usize) -> String {
        template
            .replace("{run_dir}", &run_dir.display().to_string())
            .replace("{run_index}", &run_index.to_string())
    }
}

fn is_lower_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(yaml: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kexp.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        (dir, path)
    }

    const MINIMAL: &str = r#"
id: test-kernel-1
description: "minimal kernel test"
command: "echo hello"
output: stdout
"#;

    const FILE_OUTPUT: &str = r#"
id: test-kernel-2
command: "bash kernel.sh {run_dir} {run_index}"
runs: 3
output:
  file: "{run_dir}/output-{run_index}.bin"
env:
  CUBLAS_WORKSPACE_CONFIG: ":4096:8"
hardware:
  gpu: "A100-80GB"
  cuda: "12.4"
"#;

    const HELYX_CONTRACT: &str = r#"
id: helyx-bitexact-smoke
template_version: refineforge-bitexact-v1
producer: helyx-kernels
kernel_id: helyx.bitexact.stub_v1
profile: helyx_cuda
command: "bash kernels/scripts/stub-deterministic.sh"
runs: 5
output: stdout
expected_sha256: "cd1be5aaab2e8c6846f7b87d5069142cbf595c14e8b10652b4f9a64a1a5976f3"
input_files:
  - kernels/fixtures/input-b.bin
  - kernels/fixtures/input-a.bin
tags:
  - smoke
  - helyx
env:
  CUBLAS_WORKSPACE_CONFIG: ":4096:8"
  CUDA_LAUNCH_BLOCKING: "1"
hardware:
  gpu: "RTX-3060-Laptop"
  cuda: "13.2"
  driver: "local"
"#;

    #[test]
    fn loads_minimal_with_defaults() {
        let (_d, p) = write_temp(MINIMAL);
        let exp = KernelExperiment::load(&p).unwrap();
        assert_eq!(exp.id, "test-kernel-1");
        assert_eq!(exp.runs, 5);
        assert!(matches!(exp.output, OutputSource::Stdout));
    }

    #[test]
    fn loads_file_output_with_env_and_hw() {
        let (_d, p) = write_temp(FILE_OUTPUT);
        let exp = KernelExperiment::load(&p).unwrap();
        assert_eq!(exp.runs, 3);
        match &exp.output {
            OutputSource::File(path) => assert!(path.contains("{run_dir}")),
            _ => panic!("expected File"),
        }
        assert_eq!(
            exp.env.get("CUBLAS_WORKSPACE_CONFIG"),
            Some(&":4096:8".to_string())
        );
        assert_eq!(exp.hardware.get("gpu"), Some(&"A100-80GB".to_string()));
    }

    #[test]
    fn loads_helyx_contract_fields_and_sorts_lists() {
        let (_d, p) = write_temp(HELYX_CONTRACT);
        let exp = KernelExperiment::load(&p).unwrap();
        assert_eq!(
            exp.template_version.as_deref(),
            Some("refineforge-bitexact-v1")
        );
        assert_eq!(exp.producer.as_deref(), Some("helyx-kernels"));
        assert_eq!(exp.kernel_id.as_deref(), Some("helyx.bitexact.stub_v1"));
        assert_eq!(exp.profile, KernelProfile::HelyxCuda);
        assert_eq!(
            exp.expected_sha256.as_deref(),
            Some("cd1be5aaab2e8c6846f7b87d5069142cbf595c14e8b10652b4f9a64a1a5976f3")
        );
        assert_eq!(
            exp.input_files,
            vec![
                PathBuf::from("kernels/fixtures/input-a.bin"),
                PathBuf::from("kernels/fixtures/input-b.bin"),
            ]
        );
        assert_eq!(exp.tags, vec!["helyx".to_string(), "smoke".to_string()]);
    }

    #[test]
    fn rejects_malformed_expected_sha256() {
        let yaml = HELYX_CONTRACT.replace(
            "cd1be5aaab2e8c6846f7b87d5069142cbf595c14e8b10652b4f9a64a1a5976f3",
            "not-a-sha",
        );
        let (_d, p) = write_temp(&yaml);
        let err = KernelExperiment::load(&p).unwrap_err();
        assert!(err.to_string().contains("expected_sha256"), "{err}");
    }

    #[test]
    fn rejects_runs_below_2() {
        let yaml = MINIMAL.replace("output: stdout", "output: stdout\nruns: 1");
        let (_d, p) = write_temp(&yaml);
        let err = KernelExperiment::load(&p).unwrap_err();
        assert!(err.to_string().contains("must be ≥ 2"), "{err}");
    }

    #[test]
    fn rejects_empty_command() {
        let yaml = MINIMAL.replace("\"echo hello\"", "\"\"");
        let (_d, p) = write_temp(&yaml);
        let err = KernelExperiment::load(&p).unwrap_err();
        assert!(
            err.to_string().contains("command may not be empty"),
            "{err}"
        );
    }

    #[test]
    fn substitute_replaces_tokens() {
        let (_d, p) = write_temp(MINIMAL);
        let exp = KernelExperiment::load(&p).unwrap();
        let out = exp.substitute("{run_dir}/out-{run_index}.bin", Path::new("/tmp/foo"), 3);
        assert_eq!(out, "/tmp/foo/out-3.bin");
    }
}
