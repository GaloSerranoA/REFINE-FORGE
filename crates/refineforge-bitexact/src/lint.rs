//! Kernel experiment linter for enterprise bit-exact contracts.

use serde::{Deserialize, Serialize};

use crate::experiment::{KernelExperiment, KernelProfile};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LintStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LintIssue {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LintReport {
    pub status: LintStatus,
    pub experiment_id: String,
    pub profile: KernelProfile,
    pub issues: Vec<LintIssue>,
}

pub fn lint_experiment(exp: &KernelExperiment) -> LintReport {
    let mut issues = Vec::new();
    match exp.profile {
        KernelProfile::Generic => {}
        KernelProfile::CudaStrict | KernelProfile::HelyxCuda => {
            require_opt(&mut issues, "producer", &exp.producer);
            require_opt(&mut issues, "kernel_id", &exp.kernel_id);
            require_opt(&mut issues, "expected_sha256", &exp.expected_sha256);
            if exp.runs < 5 {
                issues.push(LintIssue {
                    field: "runs".into(),
                    message: "strict CUDA profiles require runs >= 5".into(),
                });
            }
            require_map_value(
                &mut issues,
                "env.CUBLAS_WORKSPACE_CONFIG",
                exp.env.get("CUBLAS_WORKSPACE_CONFIG"),
                ":4096:8",
            );
            require_map_value(
                &mut issues,
                "env.CUDA_LAUNCH_BLOCKING",
                exp.env.get("CUDA_LAUNCH_BLOCKING"),
                "1",
            );
            require_map_key(&mut issues, "hardware.gpu", exp.hardware.get("gpu"));
            require_map_key(&mut issues, "hardware.cuda", exp.hardware.get("cuda"));
            require_map_key(&mut issues, "hardware.driver", exp.hardware.get("driver"));
        }
    }

    if exp.profile == KernelProfile::HelyxCuda {
        if exp.producer.as_deref() != Some("helyx-kernels") {
            issues.push(LintIssue {
                field: "producer".into(),
                message: "helyx_cuda profile requires producer=helyx-kernels".into(),
            });
        }
        if !exp
            .kernel_id
            .as_deref()
            .is_some_and(|kernel_id| kernel_id.starts_with("helyx."))
        {
            issues.push(LintIssue {
                field: "kernel_id".into(),
                message: "helyx_cuda profile requires kernel_id starting with `helyx.`".into(),
            });
        }
    }

    let status = if issues.is_empty() {
        LintStatus::Pass
    } else {
        LintStatus::Fail
    };
    LintReport {
        status,
        experiment_id: exp.id.clone(),
        profile: exp.profile.clone(),
        issues,
    }
}

fn require_opt(issues: &mut Vec<LintIssue>, field: &str, value: &Option<String>) {
    if value.as_deref().is_none_or(|v| v.trim().is_empty()) {
        issues.push(LintIssue {
            field: field.into(),
            message: format!("{field} is required for strict kernel profiles"),
        });
    }
}

fn require_map_key(issues: &mut Vec<LintIssue>, field: &str, value: Option<&String>) {
    if value.is_none_or(|v| v.trim().is_empty()) {
        issues.push(LintIssue {
            field: field.into(),
            message: format!("{field} is required for strict kernel profiles"),
        });
    }
}

fn require_map_value(
    issues: &mut Vec<LintIssue>,
    field: &str,
    value: Option<&String>,
    expected: &str,
) {
    if value.map(|v| v.as_str()) != Some(expected) {
        issues.push(LintIssue {
            field: field.into(),
            message: format!("{field} must be {expected:?}"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{
        KernelExperiment, KernelProduction, KernelProfile, KernelReference, KernelSource,
        OutputSource,
    };
    use std::collections::BTreeMap;

    fn exp(profile: KernelProfile) -> KernelExperiment {
        KernelExperiment {
            id: "lint-fixture".into(),
            template_version: Some("refineforge-bitexact-v1".into()),
            description: "".into(),
            producer: None,
            kernel_id: None,
            profile,
            command: "kernel".into(),
            runs: 2,
            output: OutputSource::Stdout,
            source: KernelSource::default(),
            reference: KernelReference::default(),
            production: KernelProduction::default(),
            expected_sha256: None,
            input_files: vec![],
            tags: vec![],
            env: BTreeMap::new(),
            hardware: BTreeMap::new(),
        }
    }

    #[test]
    fn generic_profile_passes_without_cuda_contract_metadata() {
        let report = lint_experiment(&exp(KernelProfile::Generic));
        assert_eq!(report.status, LintStatus::Pass);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn helyx_cuda_profile_requires_enterprise_contract_fields() {
        let report = lint_experiment(&exp(KernelProfile::HelyxCuda));
        assert_eq!(report.status, LintStatus::Fail);
        let fields: Vec<&str> = report
            .issues
            .iter()
            .map(|issue| issue.field.as_str())
            .collect();
        assert!(fields.contains(&"producer"), "{fields:?}");
        assert!(fields.contains(&"kernel_id"), "{fields:?}");
        assert!(fields.contains(&"expected_sha256"), "{fields:?}");
        assert!(fields.contains(&"runs"), "{fields:?}");
        assert!(
            fields.contains(&"env.CUBLAS_WORKSPACE_CONFIG"),
            "{fields:?}"
        );
        assert!(fields.contains(&"env.CUDA_LAUNCH_BLOCKING"), "{fields:?}");
        assert!(fields.contains(&"hardware.gpu"), "{fields:?}");
        assert!(fields.contains(&"hardware.cuda"), "{fields:?}");
        assert!(fields.contains(&"hardware.driver"), "{fields:?}");
    }

    #[test]
    fn helyx_cuda_profile_passes_valid_contract() {
        let mut exp = exp(KernelProfile::HelyxCuda);
        exp.producer = Some("helyx-kernels".into());
        exp.kernel_id = Some("helyx.bitexact.stub_v1".into());
        exp.runs = 5;
        exp.expected_sha256 =
            Some("cd1be5aaab2e8c6846f7b87d5069142cbf595c14e8b10652b4f9a64a1a5976f3".into());
        exp.env
            .insert("CUBLAS_WORKSPACE_CONFIG".into(), ":4096:8".into());
        exp.env.insert("CUDA_LAUNCH_BLOCKING".into(), "1".into());
        exp.hardware.insert("gpu".into(), "RTX-3060-Laptop".into());
        exp.hardware.insert("cuda".into(), "13.2".into());
        exp.hardware.insert("driver".into(), "local".into());

        let report = lint_experiment(&exp);

        assert_eq!(report.status, LintStatus::Pass);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
    }

    #[test]
    fn cuda_strict_does_not_require_helyx_names() {
        let mut exp = exp(KernelProfile::CudaStrict);
        exp.producer = Some("custom-kernels".into());
        exp.kernel_id = Some("custom.rope".into());
        exp.runs = 5;
        exp.expected_sha256 =
            Some("cd1be5aaab2e8c6846f7b87d5069142cbf595c14e8b10652b4f9a64a1a5976f3".into());
        exp.env
            .insert("CUBLAS_WORKSPACE_CONFIG".into(), ":4096:8".into());
        exp.env.insert("CUDA_LAUNCH_BLOCKING".into(), "1".into());
        exp.hardware.insert("gpu".into(), "A100".into());
        exp.hardware.insert("cuda".into(), "12.4".into());
        exp.hardware.insert("driver".into(), "550.54.15".into());

        assert_eq!(lint_experiment(&exp).status, LintStatus::Pass);
    }
}
