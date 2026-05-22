use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Passed,
    Failed,
    Skipped,
    Blocked,
}

impl GateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GateStatus::Passed => "passed",
            GateStatus::Failed => "failed",
            GateStatus::Skipped => "skipped",
            GateStatus::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub name: String,
    pub command: Vec<String>,
    pub status: GateStatus,
    pub required: bool,
    pub duration_ms: u128,
    pub log_path: Option<PathBuf>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum SignatureEvidence {
    Unsigned,
    Skipped {
        reason: String,
    },
    Verified {
        sigbundle: PathBuf,
        signer_identity: Option<String>,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleEvidence {
    pub claim_id: String,
    pub bundle_dir: PathBuf,
    pub manifest_sha256: String,
    pub signature: SignatureEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReport {
    pub requested_version: String,
    pub generated_at: DateTime<Utc>,
    pub git_commit: Option<String>,
    pub git_branch: Option<String>,
    pub dirty_tree: bool,
    pub host_os: String,
    pub evidence_dir: PathBuf,
    pub gates: Vec<GateReport>,
    pub bundles: Vec<BundleEvidence>,
}

impl ReleaseReport {
    pub fn new(requested_version: String, evidence_dir: PathBuf) -> Self {
        Self {
            requested_version,
            generated_at: Utc::now(),
            git_commit: None,
            git_branch: None,
            dirty_tree: false,
            host_os: std::env::consts::OS.to_string(),
            evidence_dir,
            gates: Vec::new(),
            bundles: Vec::new(),
        }
    }

    #[cfg(test)]
    fn test_fixture(version: &str) -> Self {
        Self {
            requested_version: version.into(),
            generated_at: DateTime::parse_from_rfc3339("2026-05-22T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            git_commit: Some("abc123".into()),
            git_branch: Some("codex/release-infrastructure-track".into()),
            dirty_tree: false,
            host_os: "test-os".into(),
            evidence_dir: PathBuf::from("release/evidence/test"),
            gates: Vec::new(),
            bundles: Vec::new(),
        }
    }

    pub fn required_gates_succeeded(&self) -> bool {
        self.gates
            .iter()
            .filter(|gate| gate.required)
            .all(|gate| matches!(gate.status, GateStatus::Passed | GateStatus::Skipped))
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# refineforge release readiness report\n\n");
        out.push_str(&format!("- Version: `{}`\n", self.requested_version));
        out.push_str(&format!(
            "- Generated at: `{}`\n",
            self.generated_at.to_rfc3339()
        ));
        out.push_str(&format!(
            "- Git commit: `{}`\n",
            self.git_commit.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "- Git branch: `{}`\n",
            self.git_branch.as_deref().unwrap_or("unknown")
        ));
        out.push_str(&format!("- Dirty tree: `{}`\n", self.dirty_tree));
        out.push_str(&format!("- Host OS: `{}`\n\n", self.host_os));

        out.push_str("## Gates\n\n");
        out.push_str("| Gate | Status | Required | Message |\n");
        out.push_str("|---|---|---:|---|\n");
        for gate in &self.gates {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                gate.name,
                gate.status.as_str(),
                if gate.required { "yes" } else { "no" },
                gate.message.as_deref().unwrap_or("")
            ));
        }

        out.push_str("\n## Bundles\n\n");
        out.push_str("| Claim | Manifest SHA-256 | Signature |\n");
        out.push_str("|---|---|---|\n");
        for bundle in &self.bundles {
            out.push_str(&format!(
                "| {} | `{}` | {} |\n",
                bundle.claim_id,
                bundle.manifest_sha256,
                signature_label(&bundle.signature)
            ));
        }
        out
    }
}

fn signature_label(sig: &SignatureEvidence) -> &'static str {
    match sig {
        SignatureEvidence::Unsigned => "unsigned",
        SignatureEvidence::Skipped { .. } => "skipped",
        SignatureEvidence::Verified { .. } => "verified",
        SignatureEvidence::Failed { .. } => "failed",
    }
}

pub fn gate_log_name(name: &str) -> String {
    let re = Regex::new("[^A-Za-z0-9]+").unwrap();
    let trimmed = name.trim().to_ascii_lowercase();
    let collapsed = re.replace_all(&trimmed, "-");
    collapsed.trim_matches('-').to_string() + ".log"
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut h = Sha256::new();
    h.update(data);
    Ok(hex::encode(h.finalize()))
}

pub fn write_evidence(
    report: &ReleaseReport,
    sbom: &serde_json::Value,
    provenance: &serde_json::Value,
) -> Result<()> {
    std::fs::create_dir_all(&report.evidence_dir)
        .with_context(|| format!("creating {}", report.evidence_dir.display()))?;

    let report_json = serde_json::to_vec_pretty(report)?;
    std::fs::write(report.evidence_dir.join("release-report.json"), report_json)?;
    std::fs::write(report.evidence_dir.join("release-report.md"), report.to_markdown())?;
    std::fs::write(
        report.evidence_dir.join("sbom.cyclonedx.json"),
        serde_json::to_vec_pretty(sbom)?,
    )?;
    std::fs::write(
        report.evidence_dir.join("provenance.intoto.json"),
        serde_json::to_vec_pretty(provenance)?,
    )?;
    Ok(())
}

pub fn sbom_from_cargo_metadata(
    metadata: &serde_json::Value,
    version: &str,
) -> Result<serde_json::Value> {
    let packages = metadata["packages"]
        .as_array()
        .context("cargo metadata missing packages array")?;

    let components: Vec<serde_json::Value> = packages
        .iter()
        .map(|pkg| {
            let name = pkg["name"].as_str().unwrap_or("unknown");
            let version = pkg["version"].as_str().unwrap_or("0.0.0");
            let license = pkg["license"].as_str().unwrap_or("NOASSERTION");
            serde_json::json!({
                "type": "library",
                "name": name,
                "version": version,
                "licenses": [{"license": {"id": license}}],
                "purl": format!("pkg:cargo/{name}@{version}")
            })
        })
        .collect();

    Ok(serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "refineforge",
                "version": version
            }
        },
        "components": components
    }))
}

pub fn provenance_from_report(report: &ReleaseReport) -> serde_json::Value {
    let subjects: Vec<serde_json::Value> = report
        .bundles
        .iter()
        .map(|bundle| {
            serde_json::json!({
                "name": format!("{}/manifest.json", bundle.claim_id),
                "digest": {"sha256": bundle.manifest_sha256}
            })
        })
        .collect();

    serde_json::json!({
        "_type": "https://in-toto.io/Statement/v1",
        "subject": subjects,
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "builder": {"id": "refineforge-local-release-ready"},
            "buildType": "https://github.com/galo/refineforge/release-ready",
            "invocation": {
                "parameters": {
                    "requested_version": report.requested_version
                },
                "environment": {
                    "host_os": report.host_os
                }
            },
            "materials": [
                {
                    "uri": "git+HEAD",
                    "digest": {
                        "sha1": report
                            .git_commit
                            .clone()
                            .unwrap_or_else(|| "unknown".into())
                    }
                },
                {"uri": "file:Cargo.lock"},
                {"uri": "file:lean/lean-toolchain"}
            ],
            "metadata": {
                "buildStartedOn": report.generated_at.to_rfc3339(),
                "completeness": {
                    "parameters": true,
                    "environment": false,
                    "materials": true
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gate(name: &str, status: GateStatus) -> GateReport {
        GateReport {
            name: name.to_string(),
            command: vec!["refine".into(), "dummy".into()],
            status,
            required: true,
            duration_ms: 7,
            log_path: Some(PathBuf::from(format!("logs/{name}.log"))),
            message: Some(format!("{name} message")),
        }
    }

    #[test]
    fn release_report_is_success_only_when_required_gates_pass_or_skip() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.gates.push(gate("lean-check-all", GateStatus::Passed));
        report.gates.push(gate("signature", GateStatus::Skipped));
        assert!(report.required_gates_succeeded());

        report.gates.push(gate("bundle-verify", GateStatus::Failed));
        assert!(!report.required_gates_succeeded());
    }

    #[test]
    fn blocked_required_gate_fails_release_report() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.gates.push(gate("docker-smoke", GateStatus::Blocked));
        assert!(!report.required_gates_succeeded());
    }

    #[test]
    fn markdown_report_contains_gate_table_and_bundle_hashes() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.gates.push(gate("scan-check-all", GateStatus::Passed));
        report.bundles.push(BundleEvidence {
            claim_id: "EXAMPLE-003".into(),
            bundle_dir: PathBuf::from("release/evidence/run/bundles/EXAMPLE-003"),
            manifest_sha256: "a".repeat(64),
            signature: SignatureEvidence::Unsigned,
        });

        let md = report.to_markdown();
        assert!(md.contains("# refineforge release readiness report"));
        assert!(md.contains("| scan-check-all | passed | yes |"));
        assert!(md.contains("EXAMPLE-003"));
        assert!(md.contains(&"a".repeat(64)));
    }

    #[test]
    fn gate_log_file_names_are_stable_and_windows_safe() {
        assert_eq!(
            gate_log_name("bundle verify: EXAMPLE-003"),
            "bundle-verify-example-003.log"
        );
    }

    #[test]
    fn writes_json_markdown_sbom_and_provenance_files() {
        let td = tempfile::tempdir().unwrap();
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.evidence_dir = td.path().join("evidence");
        report.gates.push(gate("lean-check-all", GateStatus::Passed));

        let sbom = serde_json::json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.5",
            "components": []
        });
        let provenance = serde_json::json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": []
        });

        write_evidence(&report, &sbom, &provenance).unwrap();

        assert!(report.evidence_dir.join("release-report.json").exists());
        assert!(report.evidence_dir.join("release-report.md").exists());
        assert!(report.evidence_dir.join("sbom.cyclonedx.json").exists());
        assert!(report.evidence_dir.join("provenance.intoto.json").exists());
    }

    #[test]
    fn sbom_from_cargo_metadata_includes_workspace_and_dependency_components() {
        let metadata = serde_json::json!({
            "packages": [
                {
                    "id": "path+file:///repo#refineforge-cli@0.2.2",
                    "name": "refineforge-cli",
                    "version": "0.2.2",
                    "license": "Apache-2.0 OR MIT",
                    "dependencies": [{"name": "serde", "req": "^1"}]
                },
                {
                    "id": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0",
                    "name": "serde",
                    "version": "1.0.0",
                    "license": "MIT OR Apache-2.0",
                    "dependencies": []
                }
            ],
            "workspace_members": ["path+file:///repo#refineforge-cli@0.2.2"]
        });

        let sbom = sbom_from_cargo_metadata(&metadata, "0.2.2").unwrap();
        assert_eq!(sbom["bomFormat"], "CycloneDX");
        assert_eq!(sbom["metadata"]["component"]["name"], "refineforge");
        let components = sbom["components"].as_array().unwrap();
        assert!(components.iter().any(|c| c["name"] == "refineforge-cli"));
        assert!(components.iter().any(|c| c["name"] == "serde"));
    }

    #[test]
    fn provenance_records_bundle_subjects_and_materials() {
        let mut report = ReleaseReport::test_fixture("0.2.2");
        report.git_commit = Some("abc123".into());
        report.bundles.push(BundleEvidence {
            claim_id: "EXAMPLE-003".into(),
            bundle_dir: PathBuf::from("bundles/EXAMPLE-003"),
            manifest_sha256: "b".repeat(64),
            signature: SignatureEvidence::Unsigned,
        });

        let provenance = provenance_from_report(&report);
        assert_eq!(provenance["_type"], "https://in-toto.io/Statement/v1");
        assert_eq!(provenance["subject"][0]["name"], "EXAMPLE-003/manifest.json");
        assert_eq!(provenance["subject"][0]["digest"]["sha256"], "b".repeat(64));
        assert_eq!(provenance["predicate"]["materials"][0]["uri"], "git+HEAD");
    }
}
