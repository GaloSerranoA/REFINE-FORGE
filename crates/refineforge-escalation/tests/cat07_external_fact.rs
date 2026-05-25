//! Category 7 — External-fact assertion. Anything not grounded
//! in the repo, claim YAMLs, or prior decision packets escalates.

use refineforge_escalation::{Action, Category, Engine, ExternalCitation, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

fn fact(a: &str, c: ExternalCitation) -> Action {
    Action::AssertExternalFact {
        assertion: a.into(),
        citation: c,
    }
}

// ---------- Positive: training-data inference ----------

#[test]
fn sha2_implements_fips_180_4_escalates() {
    let d = eng()
        .decide(
            &fact(
                "The sha2 crate implements SHA-256 per FIPS 180-4.",
                ExternalCitation::TrainingData,
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::ExternalFact));
}

// ---------- Positive: cites an RFC ----------

#[test]
fn cites_external_rfc_escalates() {
    let d = eng()
        .decide(
            &fact(
                "This matches the algorithm in RFC 5246 §6.2.3.",
                ExternalCitation::ExternalStandardNotInRepo {
                    name: "RFC 5246".into(),
                },
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: vendor claim ----------

#[test]
fn vendor_claim_escalates() {
    let d = eng()
        .decide(
            &fact(
                "The CUDA driver version 550.54.15 is bit-exact for this kernel.",
                ExternalCitation::VendorClaimNotInRepo {
                    vendor: "NVIDIA".into(),
                },
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: inferred from filename ----------

#[test]
fn inferred_from_filename_escalates() {
    let d = eng()
        .decide(
            &fact(
                "Since the file is `sha2.rs`, it must implement SHA-256.",
                ExternalCitation::InferredFromFilename,
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: no source at all ----------

#[test]
fn no_source_escalates() {
    let d = eng()
        .decide(
            &fact(
                "The HELYX production system uses Mutex<T> not RwLock<T>.",
                ExternalCitation::None,
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: cited from a repo file ----------

#[test]
fn repo_file_citation_proceeds() {
    let d = eng()
        .decide(
            &fact(
                "Per lean/lean-toolchain we pin v4.29.1.",
                ExternalCitation::RepoFile {
                    path: "lean/lean-toolchain".into(),
                },
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: cited from a claim YAML ----------

#[test]
fn claim_yaml_citation_proceeds() {
    let d = eng()
        .decide(
            &fact(
                "The claim's lean.theorems list includes `incr_increases`.",
                ExternalCitation::ClaimYaml {
                    claim_id: "EXAMPLE-002".into(),
                    field: "lean.theorems".into(),
                },
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: cited from a prior signed packet ----------

#[test]
fn prior_decision_packet_proceeds() {
    let d = eng()
        .decide(
            &fact(
                "Per packet 2026-05-10-idealisation-u64.md the operator accepted u64→Nat.",
                ExternalCitation::PriorDecisionPacket {
                    packet_id: "2026-05-10-idealisation-u64".into(),
                },
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}
