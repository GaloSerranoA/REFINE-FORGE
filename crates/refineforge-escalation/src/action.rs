//! The structured shape of every step the autonomous driver can
//! propose. Each [`Action`] variant carries the minimum data the
//! engine needs to classify it against the 9 categories.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind")]
pub enum Action {
    // ===== Lean structural changes =====
    /// Create a new Lean module (.lean file).
    AddLeanModule { path: String, imports: Vec<String> },

    /// Add a brand-new theorem to an existing module.
    AddTheorem {
        module: String,
        name: String,
        statement: String,
    },

    /// Rename an existing theorem; no semantic change.
    RenameTheorem {
        module: String,
        from: String,
        to: String,
    },

    /// Same statement, different proof body.
    RestructureProof { module: String, theorem: String },

    /// Add a test case for an already-listed theorem.
    AddTestCase {
        module: String,
        theorem: String,
        test_name: String,
    },

    /// Edit an existing theorem's statement.
    /// `weakening` is `Some` when the new statement proves
    /// strictly less than the old.
    EditTheorem {
        module: String,
        name: String,
        statement_before: String,
        statement_after: String,
        weakening: Option<WeakeningKind>,
    },

    /// Add an import to an existing module.
    AddLeanImport {
        module: String,
        import_path: String,
        is_mathlib: bool,
    },

    /// Declare a NEW axiom in our Lean source.
    WriteAxiom {
        module: String,
        axiom_name: String,
        statement: String,
    },

    // ===== Rust ↔ Lean refinement =====
    /// Map a Rust type to a Lean type. Empty `lossy_kinds`
    /// means no information is lost (e.g. String→String).
    MapRustToLean {
        rust_type: String,
        lean_type: String,
        lossy_kinds: Vec<LossKind>,
    },

    // ===== Refinement docs =====
    /// Write a sentence into `docs/refinement/<CLAIM-ID>.md`.
    /// External-fact citations use [`Action::AssertExternalFact`]
    /// instead — `kind` here covers only verifiable / repo-cited
    /// / customer-intent prose.
    WriteRefinementSentence {
        claim_id: String,
        sentence: String,
        kind: SentenceKind,
    },

    // ===== Claim YAML edits =====
    BumpClaimStatus {
        claim_id: String,
        from: ClaimStatus,
        to: ClaimStatus,
    },

    SetReviewOperator {
        claim_id: String,
        from: Option<String>,
        to: String,
    },

    // ===== External assertion (Category 7) =====
    AssertExternalFact {
        assertion: String,
        citation: ExternalCitation,
    },

    // ===== Trust-base extensions (Category 8) =====
    BumpLeanToolchain { from: String, to: String },

    AddLakePackage {
        name: String,
        version_or_rev: String,
    },

    BumpCargoPin {
        crate_name: String,
        from: String,
        to: String,
        in_bundle: bool,
        cited_in_refinement_doc: bool,
    },

    SwitchCrate {
        from: String,
        to: String,
        in_bundle: bool,
    },

    BumpCosignVersion { from: String, to: String },

    BumpGitHubActionSha {
        action: String,
        from: String,
        to: String,
    },

    AddVerifierDockerTool { tool: String },

    SwitchAnthropicModel { from: String, to: String },

    // ===== Scope-expanding additions (Category 1) =====
    AddWorkspaceCrate { name: String },

    AddTemplate { name: String },

    AddTopLevelDirectory { name: String },

    // ===== Bit-exact / kernel (Category 9) =====
    EditKernelSource { kernel_id: String, summary: String },

    ChangeKernelBuildFlags {
        kernel_id: String,
        from: String,
        to: String,
    },

    BumpKernelCompilerPin {
        kernel_id: String,
        compiler: String,
        from: String,
        to: String,
    },

    /// Reducing `run_count` below the previously-passing baseline
    /// would mask divergence — escalates per Cat 9.
    LowerBitExactRunCount {
        kernel_id: String,
        from: u32,
        to: u32,
    },

    AddKernelDirectory { kernel_id: String },

    // ===== Trivially-OK actions =====
    /// No semantic change.
    Reformat { paths: Vec<String> },

    /// Rename a local variable inside a proof body.
    RenameLocalVar {
        file: String,
        from: String,
        to: String,
    },

    /// Add `--help` text to an already-existing CLI flag.
    AddCliHelpText {
        command: String,
        flag: String,
        description: String,
    },

    // ===== Catch-all =====
    /// Any action shape the engine doesn't recognise.
    /// Per `plans/autonomous-driver-plan.md` §6 risk mitigation, this
    /// defaults to Category 1 (Scope) — never silently proceeds.
    Unknown { description: String },
}

/// The information a Rust→Lean type mapping loses. Empty list
/// means the mapping is information-preserving (e.g. bool→Bool).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LossKind {
    /// `u8..usize` → `Nat` — loses bounded / overflow semantics.
    UnsignedOverflow,
    /// `i8..isize` → `Int` — loses bit-width.
    SignedBitWidth,
    /// `[u8; N]` → `Nat`/`String` — loses fixed-width property.
    FixedWidth,
    /// `Mutex/RwLock/Arc<T>` → `T` — concurrent-access story
    /// assumed externally.
    Concurrency,
    /// `Result<T,E>` → `T` — loses failure path.
    FailurePath,
    /// `Option<T>` → `T` — loses absence.
    Absence,
    /// `f32/f64` → `Rat`/`Real` — introduces an idealisation no
    /// real CPU satisfies.
    FloatRounding,
}

/// What kind of sentence the AI wants to write into a
/// refinement doc. External-citation prose has its own dedicated
/// [`Action::AssertExternalFact`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sentence_kind")]
pub enum SentenceKind {
    /// Math statement about a Lean theorem or code description —
    /// verifiable from source.
    MachineCheckable,
    /// Cites a file already in this repo that the operator has
    /// previously approved.
    RepoCitable { source_path: String },
    /// Claim about what customers / users / regulators / operators
    /// understand. Always escalates per Category 4.
    CustomerIntent,
}

/// Source the AI used for an external-fact assertion. Determines
/// whether Category 7 (External-fact) fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "citation_kind")]
pub enum ExternalCitation {
    /// No source — AI made it up. Always escalates.
    None,
    /// Drawn from training data; not verifiable from repo. Escalates.
    TrainingData,
    /// Inferred from a filename or symbol name; not verifiable. Escalates.
    InferredFromFilename,
    /// External standard / RFC / paper text whose body the AI
    /// hasn't been given. Escalates.
    ExternalStandardNotInRepo { name: String },
    /// Vendor claim the AI cannot verify. Escalates.
    VendorClaimNotInRepo { vendor: String },
    /// A file already in this repo. Proceed.
    RepoFile { path: String },
    /// A field in a claim YAML. Proceed.
    ClaimYaml { claim_id: String, field: String },
    /// A prior decision packet the operator signed. Proceed.
    PriorDecisionPacket { packet_id: String },
}

impl ExternalCitation {
    /// True when the citation grounds the assertion in something
    /// already in the repo. Category 7 fires when this is false.
    pub fn is_repo_grounded(&self) -> bool {
        matches!(
            self,
            Self::RepoFile { .. } | Self::ClaimYaml { .. } | Self::PriorDecisionPacket { .. }
        )
    }
}

/// How a theorem edit weakens the original statement (Category 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeakeningKind {
    /// `∀ x, P x` → `∀ x ∈ S, P x` (added hypothesis / shrank domain).
    AddedHypothesis,
    /// `P ∧ Q` → `P` (dropped conjunct).
    DroppedConjunct,
    /// `>` → `≥` (strict → non-strict).
    StrictToNonStrict,
    /// Catch-all for anything else that proves strictly less.
    GeneralReplacement,
}

/// The five claim states refineforge recognises in claim YAML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Unformalized,
    Drafted,
    Broken,
    ProvenModelOnly,
    ProvenModelAndRefined,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_citation_repo_grounded_three_variants() {
        let cases = [
            ExternalCitation::RepoFile {
                path: "docs/methodology.md".into(),
            },
            ExternalCitation::ClaimYaml {
                claim_id: "EXAMPLE-001".into(),
                field: "lean.theorems".into(),
            },
            ExternalCitation::PriorDecisionPacket {
                packet_id: "2026-05-18-idealisation-u64".into(),
            },
        ];
        for c in &cases {
            assert!(c.is_repo_grounded(), "expected grounded: {:?}", c);
        }
    }

    #[test]
    fn external_citation_not_grounded_five_variants() {
        let cases = [
            ExternalCitation::None,
            ExternalCitation::TrainingData,
            ExternalCitation::InferredFromFilename,
            ExternalCitation::ExternalStandardNotInRepo {
                name: "RFC 5246".into(),
            },
            ExternalCitation::VendorClaimNotInRepo {
                vendor: "NVIDIA".into(),
            },
        ];
        for c in &cases {
            assert!(!c.is_repo_grounded(), "expected NOT grounded: {:?}", c);
        }
    }

    #[test]
    fn action_serde_round_trip_simple() {
        let a = Action::AddTheorem {
            module: "Refineforge.Counter".into(),
            name: "t1".into(),
            statement: "∀ c, P c".into(),
        };
        let j = serde_json::to_string(&a).expect("ser");
        let back: Action = serde_json::from_str(&j).expect("de");
        assert_eq!(back, a);
    }

    #[test]
    fn action_serde_round_trip_with_optional_weakening() {
        let a = Action::EditTheorem {
            module: "Refineforge.Counter".into(),
            name: "monotone".into(),
            statement_before: "(incr c).value > c.value".into(),
            statement_after: "(incr c).value ≥ c.value".into(),
            weakening: Some(WeakeningKind::StrictToNonStrict),
        };
        let j = serde_json::to_string(&a).expect("ser");
        let back: Action = serde_json::from_str(&j).expect("de");
        assert_eq!(back, a);
    }

    #[test]
    fn claim_status_serdes_snake_case() {
        let j = serde_json::to_string(&ClaimStatus::ProvenModelAndRefined).expect("ser");
        assert!(j.contains("proven_model_and_refined"), "got {}", j);
    }
}
