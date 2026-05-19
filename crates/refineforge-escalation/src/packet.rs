//! Decision packet — the markdown artifact the autonomous
//! driver writes into `escalations/<CLAIM-ID>/` for the
//! operator to approve, reject, or edit-and-resubmit.
//!
//! Layout:
//!
//! ```text
//! ---
//! criteria_version: "0.3"
//! claim_id: EXAMPLE-002
//! category: idealisation
//! all_categories: [idealisation]
//! generated_at: 2026-05-18T20:30:45Z
//! generated_by_strategy: anthropic
//! batch: null   # or { items: [...], rationale_for_batching: "..." }
//! ---
//!
//! # Escalation: <summary>
//!
//! ## Why this escalates
//! ...per-category sections...
//!
//! ## AI's recommendation
//! ...
//!
//! ## Human decision
//! <!-- the operator fills this in and commits -->
//! ```
//!
//! Per criteria-doc v0.3: there is NO `expires_at` field
//! (auto-expiry was rejected). The driver waits indefinitely
//! on each packet.

use crate::action::Action;
use crate::category::Category;
use crate::decision::{EscalationReason, Evidence};
use crate::CRITERIA_VERSION;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// The full decision packet. Renders to a single markdown file
/// (YAML front-matter + body) via [`Packet::to_markdown`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Packet {
    pub front_matter: PacketFrontMatter,
    /// Headline summary the operator sees in `refine escalations list`.
    pub summary: String,
    /// Structured evidence — see [`crate::Evidence`] variants.
    pub evidence: Evidence,
    /// The raw action the AI proposes (for traceability).
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketFrontMatter {
    pub criteria_version: String,
    pub claim_id: String,
    pub category: Category,
    pub all_categories: Vec<Category>,
    pub generated_at: String,
    pub generated_by_strategy: String,
    /// `None` for the default one-item-per-packet case; `Some`
    /// when the AI is proposing a v0.3 batch.
    pub batch: Option<BatchBlock>,
}

/// v0.3: the AI may propose batching when (a) every item trips
/// the same category, (b) analysis is identical, (c) evidence
/// doesn't distinguish items. The `rationale_for_batching`
/// field is itself reviewable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchBlock {
    pub items: Vec<BatchItem>,
    pub rationale_for_batching: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchItem {
    /// Stable id the operator references in partial-approval
    /// responses (e.g. `counter.value_u64_to_nat`).
    pub id: String,
    pub summary: String,
    pub evidence: Evidence,
}

impl Packet {
    /// Build a packet from an engine [`EscalationReason`] +
    /// the originating action + driver metadata.
    pub fn build(
        reason: EscalationReason,
        action: Action,
        claim_id: impl Into<String>,
        generated_at: impl Into<String>,
        generated_by_strategy: impl Into<String>,
    ) -> Self {
        Self {
            front_matter: PacketFrontMatter {
                criteria_version: CRITERIA_VERSION.into(),
                claim_id: claim_id.into(),
                category: reason.primary,
                all_categories: reason.categories,
                generated_at: generated_at.into(),
                generated_by_strategy: generated_by_strategy.into(),
                batch: None,
            },
            summary: reason.summary,
            evidence: reason.evidence,
            action,
        }
    }

    /// Attach a [`BatchBlock`] to a packet. Caller is
    /// responsible for verifying the v0.3 batching conditions
    /// (same category / identical analysis / undifferentiated
    /// evidence) before calling.
    pub fn with_batch(mut self, batch: BatchBlock) -> Self {
        self.front_matter.batch = Some(batch);
        self
    }

    /// Render the full markdown file the autonomous driver
    /// commits to `escalations/<CLAIM-ID>/<filename>.md`.
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("---\n");
        // serde_yaml inserts a leading `---\n` of its own; strip the
        // outer doc-start to avoid `--- --- yaml ---`.
        let yaml = serde_yaml::to_string(&self.front_matter)
            .expect("PacketFrontMatter always serializes");
        s.push_str(yaml.trim_start_matches("---\n"));
        s.push_str("---\n\n");

        let _ = writeln!(s, "# Escalation: {}", self.summary);
        let _ = writeln!(s);

        let _ = writeln!(s, "**Category:** {}", self.front_matter.category);
        if self.front_matter.all_categories.len() > 1 {
            let others: Vec<String> = self
                .front_matter
                .all_categories
                .iter()
                .filter(|c| **c != self.front_matter.category)
                .map(|c| format!("{}", c))
                .collect();
            let _ = writeln!(s, "**Also trips:** {}", others.join(", "));
        }
        let _ = writeln!(s, "**Claim:** `{}`", self.front_matter.claim_id);
        let _ = writeln!(
            s,
            "**Generated:** {} by `{}`",
            self.front_matter.generated_at, self.front_matter.generated_by_strategy
        );
        let _ = writeln!(s, "**Criteria version:** v{}", self.front_matter.criteria_version);
        let _ = writeln!(s);

        let _ = writeln!(s, "## Why this escalates");
        let _ = writeln!(s);
        render_evidence(&mut s, &self.evidence);
        let _ = writeln!(s);

        let _ = writeln!(s, "## Proposed action (raw)");
        let _ = writeln!(s);
        let _ = writeln!(s, "```json");
        let _ = writeln!(
            s,
            "{}",
            serde_json::to_string_pretty(&self.action)
                .unwrap_or_else(|_| "{}".into())
        );
        let _ = writeln!(s, "```");
        let _ = writeln!(s);

        if let Some(batch) = &self.front_matter.batch {
            let _ = writeln!(s, "## Batched items ({} total)", batch.items.len());
            let _ = writeln!(s);
            let _ = writeln!(
                s,
                "**Rationale for batching:** {}",
                batch.rationale_for_batching
            );
            let _ = writeln!(s);
            let _ = writeln!(s, "Per v0.3 conditions: same category, identical");
            let _ = writeln!(s, "analysis, undifferentiated evidence. If any of");
            let _ = writeln!(s, "those is wrong, REJECT or partial-approve.");
            let _ = writeln!(s);
            for (i, item) in batch.items.iter().enumerate() {
                let _ = writeln!(s, "### Item {} — `{}`", i + 1, item.id);
                let _ = writeln!(s);
                let _ = writeln!(s, "{}", item.summary);
                let _ = writeln!(s);
                render_evidence(&mut s, &item.evidence);
                let _ = writeln!(s);
            }
        }

        let _ = writeln!(s, "## Human decision");
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "<!-- The operator fills this section in and commits. -->"
        );
        let _ = writeln!(
            s,
            "<!-- Recognised forms: `APPROVED: <reason>` / `REJECTED: <reason>` / `EDIT_AND_RESUBMIT: <suggestions>` -->"
        );
        if self.front_matter.batch.is_some() {
            let _ = writeln!(
                s,
                "<!-- Partial form (batched only): `APPROVED: 1-5,7; REJECTED: 6,8 [reasons]` -->"
            );
        }
        let _ = writeln!(s);
        let _ = writeln!(s, "(pending)");
        s
    }
}

fn render_evidence(s: &mut String, ev: &Evidence) {
    match ev {
        Evidence::Scope {
            what_added,
            smallest_in_scope_alternative,
        } => {
            let _ = writeln!(s, "- **What's being added (scope-expansion):** {}", what_added);
            if let Some(alt) = smallest_in_scope_alternative {
                let _ = writeln!(s, "- **Smallest in-scope alternative considered:** {}", alt);
            } else {
                let _ = writeln!(
                    s,
                    "- **Smallest in-scope alternative considered:** _none — AI proposes adding the new entity directly._"
                );
            }
        }
        Evidence::Idealisation {
            rust_type,
            lean_type,
            lost_properties,
        } => {
            let _ = writeln!(s, "- **Rust type:** `{}`", rust_type);
            let _ = writeln!(s, "- **Lean type:** `{}`", lean_type);
            let _ = writeln!(s, "- **Information lost in mapping:**");
            for k in lost_properties {
                let _ = writeln!(s, "  - `{:?}`", k);
            }
            let _ = writeln!(
                s,
                "- **Decision required:** does the lost information matter for this claim's stated theorems? If yes, the AI must propose a preserving mapping (e.g. `BitVec N`). If no, document why."
            );
        }
        Evidence::CustomAxiom {
            module,
            axiom_name,
            statement,
        } => {
            let _ = writeln!(s, "- **Module:** `{}`", module);
            let _ = writeln!(s, "- **Axiom name:** `{}`", axiom_name);
            let _ = writeln!(s, "- **Statement:**");
            let _ = writeln!(s);
            let _ = writeln!(s, "  ```lean");
            let _ = writeln!(s, "  axiom {} : {}", axiom_name, statement);
            let _ = writeln!(s, "  ```");
            let _ = writeln!(
                s,
                "- **Required:** the operator must either (a) override `policy.no_axioms_beyond_lean_core` in the claim YAML, or (b) instruct the AI to discharge the axiom via a proof from stronger assumptions."
            );
        }
        Evidence::CustomerIntent { claim_id, sentence } => {
            let _ = writeln!(s, "- **Claim:** `{}`", claim_id);
            let _ = writeln!(s, "- **Sentence the AI wants to write:**");
            let _ = writeln!(s);
            let _ = writeln!(s, "  > {}", sentence);
            let _ = writeln!(s);
            let _ = writeln!(
                s,
                "- **Required:** the operator must either (a) confirm the customer-intent claim is accurate, (b) propose a weaker framing, or (c) reject — the AI cannot have met any customers / users / regulators."
            );
        }
        Evidence::StatusUpgrade { claim_id, from, to } => {
            let _ = writeln!(s, "- **Claim:** `{}`", claim_id);
            let _ = writeln!(s, "- **Current:** `{}`", from);
            let _ = writeln!(s, "- **Proposed:** `{}`", to);
            let _ = writeln!(
                s,
                "- **Required:** every `[needs human]` reviewer-checklist item must be addressed and every machine-checkable item passing before approval."
            );
        }
        Evidence::TheoremWeakening {
            module,
            theorem,
            statement_before,
            statement_after,
            weakening,
        } => {
            let _ = writeln!(s, "- **Module:** `{}`", module);
            let _ = writeln!(s, "- **Theorem:** `{}`", theorem);
            let _ = writeln!(s, "- **Weakening kind:** `{:?}`", weakening);
            let _ = writeln!(s, "- **Original statement:**");
            let _ = writeln!(s);
            let _ = writeln!(s, "  ```lean");
            let _ = writeln!(s, "  theorem {} : {}", theorem, statement_before);
            let _ = writeln!(s, "  ```");
            let _ = writeln!(s);
            let _ = writeln!(s, "- **Proposed weaker statement:**");
            let _ = writeln!(s);
            let _ = writeln!(s, "  ```lean");
            let _ = writeln!(s, "  theorem {} : {}", theorem, statement_after);
            let _ = writeln!(s, "  ```");
            let _ = writeln!(
                s,
                "- **Required:** the operator must confirm the weaker statement still satisfies every claim that cites the theorem; sibling claims may need updating."
            );
        }
        Evidence::ExternalFact {
            assertion,
            citation,
        } => {
            let _ = writeln!(s, "- **Assertion the AI wants to make:**");
            let _ = writeln!(s);
            let _ = writeln!(s, "  > {}", assertion);
            let _ = writeln!(s);
            let _ = writeln!(s, "- **AI's source:** `{:?}`", citation);
            let _ = writeln!(
                s,
                "- **Required:** the operator must either (a) confirm the external citation, (b) instruct the AI to weaken the wording, or (c) reject."
            );
        }
        Evidence::TrustBaseExtension { what, from, to } => {
            let _ = writeln!(s, "- **What's changing in the trust base:** {}", what);
            if let Some(f) = from {
                let _ = writeln!(s, "- **From:** `{}`", f);
            }
            let _ = writeln!(s, "- **To:** `{}`", to);
            let _ = writeln!(
                s,
                "- **Required:** the operator must document the transitive change-set, the reason for the bump, sibling claims needing re-verification, and whether already-signed bundles are invalidated. Mathlib/Lake first-use additionally requires naming the specific modules + the lake-manifest.json diff + the reviewer-checklist update."
            );
        }
        Evidence::BitExactRegression {
            kernel_id,
            change_summary,
        } => {
            let _ = writeln!(s, "- **Kernel:** `{}`", kernel_id);
            let _ = writeln!(s, "- **Change:** {}", change_summary);
            let _ = writeln!(
                s,
                "- **Required:** prior gate state (passing/failing), predicted bit-impact (no change / re-baseline / unknown-without-hardware), hardware classes verified vs not."
            );
        }
        Evidence::UnknownActionShape { description } => {
            let _ = writeln!(s, "- **Unrecognised AI action shape:**");
            let _ = writeln!(s);
            let _ = writeln!(s, "  > {}", description);
            let _ = writeln!(s);
            let _ = writeln!(
                s,
                "- **Required:** per plan §6, unrecognised shapes default to Cat 1 (Scope). The operator either (a) approves the proposed action, (b) instructs the AI to reformulate it as a recognised `Action` variant, or (c) extends the `Action` enum in `crates/refineforge-escalation/src/action.rs` and re-runs."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::LossKind;

    fn frontmatter_block(md: &str) -> &str {
        let body = md.strip_prefix("---\n").expect("front-matter start");
        let end = body.find("\n---\n").expect("front-matter end");
        &body[..end]
    }

    fn build_idealisation_packet() -> Packet {
        let reason = EscalationReason {
            categories: vec![Category::Idealisation],
            primary: Category::Idealisation,
            summary: "u64 → Nat loses overflow".into(),
            evidence: Evidence::Idealisation {
                rust_type: "u64".into(),
                lean_type: "Nat".into(),
                lost_properties: vec![LossKind::UnsignedOverflow],
            },
        };
        let action = Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![LossKind::UnsignedOverflow],
        };
        Packet::build(
            reason,
            action,
            "EXAMPLE-002",
            "2026-05-18T20:30:45Z",
            "anthropic",
        )
    }

    #[test]
    fn build_sets_criteria_version_to_engine_constant() {
        let p = build_idealisation_packet();
        assert_eq!(p.front_matter.criteria_version, CRITERIA_VERSION);
    }

    #[test]
    fn build_carries_primary_and_all_categories() {
        let p = build_idealisation_packet();
        assert_eq!(p.front_matter.category, Category::Idealisation);
        assert_eq!(p.front_matter.all_categories, vec![Category::Idealisation]);
    }

    #[test]
    fn build_has_no_batch_by_default() {
        let p = build_idealisation_packet();
        assert!(p.front_matter.batch.is_none());
    }

    #[test]
    fn with_batch_attaches_block() {
        let p = build_idealisation_packet().with_batch(BatchBlock {
            items: vec![BatchItem {
                id: "counter.value".into(),
                summary: "u64 → Nat".into(),
                evidence: Evidence::Idealisation {
                    rust_type: "u64".into(),
                    lean_type: "Nat".into(),
                    lost_properties: vec![LossKind::UnsignedOverflow],
                },
            }],
            rationale_for_batching: "single u64 field".into(),
        });
        assert!(p.front_matter.batch.is_some());
        assert_eq!(p.front_matter.batch.as_ref().unwrap().items.len(), 1);
    }

    #[test]
    fn to_markdown_starts_with_yaml_front_matter() {
        let md = build_idealisation_packet().to_markdown();
        assert!(md.starts_with("---\n"), "got: {:?}", &md[..40]);
        // a second `---\n` ends the front matter (after the YAML body)
        let after_first = &md[4..];
        assert!(after_first.contains("\n---\n"), "no front-matter end");
    }

    #[test]
    fn to_markdown_contains_summary_as_h1() {
        let md = build_idealisation_packet().to_markdown();
        assert!(md.contains("# Escalation: u64 → Nat loses overflow"), "got:\n{}", md);
    }

    #[test]
    fn to_markdown_contains_human_decision_section_with_pending() {
        let md = build_idealisation_packet().to_markdown();
        assert!(md.contains("## Human decision"));
        assert!(md.contains("(pending)"));
        // v0.3: no `expires_at` anywhere — auto-expiry was rejected
        assert!(!md.contains("expires_at"), "v0.3 forbids expires_at: {}", md);
        assert!(!md.contains("EXPIRED-AUTO-REJECTED"));
    }

    #[test]
    fn to_markdown_lists_secondary_categories_when_multi_trip() {
        let reason = EscalationReason {
            categories: vec![Category::Scope, Category::BitExactRegression],
            primary: Category::BitExactRegression,
            summary: "add kernels/rope_v2/".into(),
            evidence: Evidence::BitExactRegression {
                kernel_id: "rope_v2".into(),
                change_summary: "new un-baselined target".into(),
            },
        };
        let action = Action::AddKernelDirectory {
            kernel_id: "rope_v2".into(),
        };
        let p = Packet::build(reason, action, "EXAMPLE-002", "t", "anthropic");
        let md = p.to_markdown();
        assert!(md.contains("Also trips"), "missing secondary section:\n{}", md);
        assert!(md.contains("scope"), "secondary not listed: {}", md);
    }

    #[test]
    fn to_markdown_batched_section_appears_when_batch_set() {
        let p = build_idealisation_packet().with_batch(BatchBlock {
            items: vec![
                BatchItem {
                    id: "counter.value".into(),
                    summary: "u64 → Nat at value".into(),
                    evidence: Evidence::Idealisation {
                        rust_type: "u64".into(),
                        lean_type: "Nat".into(),
                        lost_properties: vec![LossKind::UnsignedOverflow],
                    },
                },
                BatchItem {
                    id: "counter.timestamp".into(),
                    summary: "u64 → Nat at timestamp".into(),
                    evidence: Evidence::Idealisation {
                        rust_type: "u64".into(),
                        lean_type: "Nat".into(),
                        lost_properties: vec![LossKind::UnsignedOverflow],
                    },
                },
            ],
            rationale_for_batching: "same struct, same trade-off".into(),
        });
        let md = p.to_markdown();
        assert!(md.contains("## Batched items (2 total)"));
        assert!(md.contains("counter.value"));
        assert!(md.contains("counter.timestamp"));
        assert!(md.contains("Rationale for batching"));
        // partial-approval form is documented in the comment block
        assert!(md.contains("Partial form"));
    }

    #[test]
    fn front_matter_round_trips_via_yaml() {
        let md = build_idealisation_packet().to_markdown();
        let block = frontmatter_block(&md);
        let back: PacketFrontMatter = serde_yaml::from_str(block).expect("yaml deserialize");
        assert_eq!(back.criteria_version, CRITERIA_VERSION);
        assert_eq!(back.claim_id, "EXAMPLE-002");
        assert_eq!(back.category, Category::Idealisation);
        assert_eq!(back.generated_by_strategy, "anthropic");
        assert!(back.batch.is_none());
    }

    #[test]
    fn render_evidence_dispatches_for_each_variant() {
        // Sanity check: every Evidence variant produces non-empty
        // markdown without panicking. Add new variants here.
        let evidences = [
            Evidence::Scope {
                what_added: "x".into(),
                smallest_in_scope_alternative: None,
            },
            Evidence::Idealisation {
                rust_type: "u64".into(),
                lean_type: "Nat".into(),
                lost_properties: vec![LossKind::UnsignedOverflow],
            },
            Evidence::CustomAxiom {
                module: "M".into(),
                axiom_name: "a".into(),
                statement: "True".into(),
            },
            Evidence::CustomerIntent {
                claim_id: "C".into(),
                sentence: "Users expect X".into(),
            },
            Evidence::StatusUpgrade {
                claim_id: "C".into(),
                from: "a".into(),
                to: "b".into(),
            },
            Evidence::TheoremWeakening {
                module: "M".into(),
                theorem: "t".into(),
                statement_before: "P".into(),
                statement_after: "Q".into(),
                weakening: crate::WeakeningKind::StrictToNonStrict,
            },
            Evidence::ExternalFact {
                assertion: "RFC X says Y".into(),
                citation: crate::ExternalCitation::None,
            },
            Evidence::TrustBaseExtension {
                what: "cosign".into(),
                from: Some("v2.4.1".into()),
                to: "v2.5.0".into(),
            },
            Evidence::BitExactRegression {
                kernel_id: "k".into(),
                change_summary: "edit".into(),
            },
            Evidence::UnknownActionShape {
                description: "novel step".into(),
            },
        ];
        for ev in &evidences {
            let mut s = String::new();
            render_evidence(&mut s, ev);
            assert!(!s.is_empty(), "empty render for {:?}", ev);
            assert!(s.contains("- **"), "missing bullet marker for {:?}", ev);
        }
    }
}
