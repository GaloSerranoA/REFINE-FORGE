//! HRM-Text runtime factory surface.
//!
//! Refine-Forge owns heavyweight runtime production for HELYX: PyTorch/CUDA
//! probes, HRM-Text checkpoint manifests, and hash-stable handoff evidence.
//! HELYX consumes these manifests; it does not embed checkpoint blobs in core.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ProbeOptions {
    pub source_repo: PathBuf,
    pub python: String,
    pub out: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ManifestOptions {
    pub checkpoint_dir: PathBuf,
    pub source_repo: PathBuf,
    pub config_file: Option<PathBuf>,
    pub tokenizer_file: Option<PathBuf>,
    pub out: PathBuf,
}

pub fn write_probe(opts: &ProbeOptions) -> Result<Value> {
    let report = build_probe(opts)?;
    write_json(&opts.out, &report)?;
    Ok(report)
}

pub fn write_manifest(opts: &ManifestOptions) -> Result<Value> {
    let manifest = build_manifest(opts)?;
    write_json(&opts.out, &manifest)?;
    Ok(manifest)
}

fn build_probe(opts: &ProbeOptions) -> Result<Value> {
    let source = source_repo_json(&opts.source_repo);
    let mut blockers = source_blockers(&source);
    let python_probe = run_python_probe(&opts.python);
    if let Some(blocker) = python_probe.get("blocker").and_then(Value::as_str) {
        blockers.push(blocker.to_string());
    }
    if python_probe.get("torch_available").and_then(Value::as_bool) == Some(false) {
        blockers.push("torch is not importable".to_string());
    }
    if python_probe
        .get("flash_attention_available")
        .and_then(Value::as_bool)
        == Some(false)
    {
        blockers.push("FlashAttention is not importable".to_string());
    }
    let status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    Ok(json!({
        "schema_version": "refineforge-hrm-text-runtime-probe-v1",
        "status": status,
        "source_project": "HRM-Text",
        "source_license": "Apache-2.0",
        "source_repo": source,
        "python": python_probe,
        "blockers": blockers,
        "public_claim": "hrm_text_runtime_probe_only_no_checkpoint_claim"
    }))
}

fn build_manifest(opts: &ManifestOptions) -> Result<Value> {
    let source = source_repo_json(&opts.source_repo);
    let mut blockers = source_blockers(&source);
    if !opts.checkpoint_dir.exists() {
        blockers.push(format!(
            "checkpoint_dir does not exist: {}",
            opts.checkpoint_dir.display()
        ));
    }
    let checkpoint_files = checkpoint_files(&opts.checkpoint_dir)?;
    if checkpoint_files.is_empty() {
        blockers.push("checkpoint_dir has no recognized HRM-Text checkpoint files".to_string());
    }
    let checkpoint_sha256 = hash_file_entries(&opts.checkpoint_dir, &checkpoint_files)?;

    let config = optional_artifact("config", opts.config_file.as_deref())?;
    if config
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "missing")
    {
        blockers.push("config_file is missing".to_string());
    }
    let tokenizer = optional_artifact("tokenizer", opts.tokenizer_file.as_deref())?;
    if tokenizer
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "missing")
    {
        blockers.push("tokenizer_file is missing".to_string());
    }

    let status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    let files = checkpoint_files
        .iter()
        .map(|file| file_entry(&opts.checkpoint_dir, file))
        .collect::<Result<Vec<_>>>()?;
    let manifest = json!({
        "schema_version": "refineforge-hrm-text-runtime-manifest-v1",
        "status": status,
        "source_project": "HRM-Text",
        "source_license": "Apache-2.0",
        "source_repo": source,
        "checkpoint": {
            "dir": normalize_path(&opts.checkpoint_dir),
            "file_count": files.len(),
            "sha256": checkpoint_sha256,
            "files": files,
        },
        "config": config,
        "tokenizer": tokenizer,
        "blockers": blockers,
        "helyx_handoff": {
            "requires_hash_verification": true,
            "adapter": "helyx-inference::hrm_text",
            "policy": "refuse_missing_or_hash_mismatched_artifacts"
        },
        "public_claim": "hrm_text_runtime_artifacts_manifested_not_embedded_in_helyx_core"
    });
    Ok(manifest)
}

fn source_repo_json(source_repo: &Path) -> Value {
    json!({
        "path": normalize_path(source_repo),
        "exists": source_repo.exists(),
        "pretrain_py_exists": source_repo.join("pretrain.py").exists(),
        "simple_inference_engine_py_exists": source_repo.join("simple_inference_engine.py").exists(),
        "convert_to_hf_exists": source_repo.join("conversion").join("convert_to_hf.py").exists(),
    })
}

fn source_blockers(source: &Value) -> Vec<String> {
    let mut blockers = Vec::new();
    if source.get("exists").and_then(Value::as_bool) != Some(true) {
        blockers.push("source_repo does not exist".to_string());
    }
    if source.get("pretrain_py_exists").and_then(Value::as_bool) != Some(true) {
        blockers.push("source_repo is missing pretrain.py".to_string());
    }
    blockers
}

fn run_python_probe(python: &str) -> Value {
    let script = r#"
import importlib.util, json
out = {
    "torch_available": importlib.util.find_spec("torch") is not None,
    "flash_attention_available": (
        importlib.util.find_spec("flash_attn") is not None
        or importlib.util.find_spec("flash_attn_interface") is not None
    ),
    "cuda_available": None,
    "torch_version": None,
}
if out["torch_available"]:
    import torch
    out["cuda_available"] = bool(torch.cuda.is_available())
    out["torch_version"] = getattr(torch, "__version__", None)
print(json.dumps(out, sort_keys=True))
"#;
    match Command::new(python).arg("-c").arg(script).output() {
        Ok(output) if output.status.success() => serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| {
                json!({
                    "command": python,
                    "executed": true,
                    "blocker": format!("python probe output was not JSON: {error}"),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string()
                })
            }),
        Ok(output) => json!({
            "command": python,
            "executed": true,
            "blocker": format!("python probe command failed with status {}", output.status),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string()
        }),
        Err(error) => json!({
            "command": python,
            "executed": false,
            "blocker": format!("python probe command failed: {error}")
        }),
    }
}

fn checkpoint_files(checkpoint_dir: &Path) -> Result<Vec<PathBuf>> {
    if !checkpoint_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(checkpoint_dir).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() && is_checkpoint_artifact(entry.path()) {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort_by_key(|path| normalize_path(path));
    Ok(files)
}

fn is_checkpoint_artifact(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".safetensors")
        || lower.ends_with(".bin")
        || lower.ends_with(".pt")
        || lower.ends_with(".pth")
        || lower.ends_with(".ckpt")
        || lower.ends_with(".npy")
        || lower.ends_with(".npz")
        || lower.starts_with("fsdp_epoch_")
        || lower.starts_with("carry_epoch_")
}

fn file_entry(root: &Path, path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let role = artifact_role(path);
    Ok(json!({
        "role": role,
        "path": normalize_path(path.strip_prefix(root).unwrap_or(path)),
        "sha256": hex_sha256(&bytes),
        "size_bytes": bytes.len()
    }))
}

fn artifact_role(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.starts_with("carry_epoch_") {
        "carry_state"
    } else if name.contains("optimizer") {
        "optimizer_state"
    } else {
        "checkpoint_shard"
    }
}

fn hash_file_entries(root: &Path, files: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha256::new();
    for file in files {
        let relative = file.strip_prefix(root).unwrap_or(file);
        hasher.update(normalize_path(relative).as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(file)?);
        hasher.update([0xff]);
    }
    Ok(hex_digest(hasher))
}

fn optional_artifact(label: &str, path: Option<&Path>) -> Result<Value> {
    let Some(path) = path else {
        return Ok(json!({"status": "not_provided"}));
    };
    if !path.exists() {
        return Ok(json!({
            "status": "missing",
            "path": normalize_path(path)
        }));
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(json!({
        "status": "present",
        "role": label,
        "path": normalize_path(path),
        "sha256": hex_sha256(&bytes),
        "size_bytes": bytes.len()
    }))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", path.display()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher)
}

fn hex_digest(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
