//! Per-epoch prompt-template sampler for the Mathlib-mutation corpus.
//!
//! For each `(row_id, epoch)` pair, deterministically picks one template
//! from the library's satisfiable subset — i.e., templates whose
//! [`TemplateRequirements`] are met by the row's declared
//! [`AvailableState`].
//!
//! Inspired by InstructGLM's multi-prompt instruction-tuning loop
//! (Ye et al., EACL 2024): the same `(input, target)` pair is presented
//! through several phrasings across epochs so the fine-tune doesn't
//! overfit to one prompt format. Sampling is deterministic (SHA-256 over
//! `seed || row_id || epoch`) so a training run is byte-reproducible.
//!
//! Owned by Section 2: ML / training engineer. Designed to be called
//! at pack time (in `pack.rs` or similar) to plan which template each
//! emitted `(prompt, target)` pair uses; the template id is logged
//! alongside each pair for per-template eval-time attribution.

use refineforge_repair_api::proof_graph::{PromptTemplate, TemplateLibrary, TemplateRequirements};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ─── Capability bitset ──────────────────────────────────────────────────

/// What the extractor was able to fill for a given row. Compared
/// against each template's [`TemplateRequirements`] to decide which
/// templates are even candidates for that row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableState {
    #[serde(default)]
    pub has_goal: bool,
    #[serde(default)]
    pub has_hypotheses: bool,
    #[serde(default)]
    pub has_tactic_history: bool,
    #[serde(default)]
    pub has_lemma_neighborhood: bool,
}

impl AvailableState {
    /// Convenience constructor for fully-extracted rows (all four
    /// graph pieces present).
    pub fn all_present() -> Self {
        Self {
            has_goal: true,
            has_hypotheses: true,
            has_tactic_history: true,
            has_lemma_neighborhood: true,
        }
    }

    pub fn satisfies(&self, requires: &TemplateRequirements) -> bool {
        (!requires.needs_goal || self.has_goal)
            && (!requires.needs_hypotheses || self.has_hypotheses)
            && (!requires.needs_tactic_history || self.has_tactic_history)
            && (!requires.needs_lemma_neighborhood || self.has_lemma_neighborhood)
    }
}

// ─── Sampler ────────────────────────────────────────────────────────────

pub struct Sampler<'a> {
    library: &'a TemplateLibrary,
    seed: u64,
}

impl<'a> Sampler<'a> {
    pub fn new(library: &'a TemplateLibrary, seed: u64) -> Self {
        Self { library, seed }
    }

    /// Pick one template for `(row_id, epoch)` from the templates
    /// `available` satisfies. Returns `None` only when no template is
    /// satisfiable for the row (extractor too poor for any template's
    /// requirements).
    pub fn pick(
        &self,
        row_id: &str,
        epoch: u32,
        available: &AvailableState,
    ) -> Option<&'a PromptTemplate> {
        let candidates: Vec<&'a PromptTemplate> = self
            .library
            .templates
            .iter()
            .filter(|t| available.satisfies(&t.requires))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let idx = self.hash_index(row_id, epoch, candidates.len());
        Some(candidates[idx])
    }

    fn hash_index(&self, row_id: &str, epoch: u32, modulus: usize) -> usize {
        let mut hasher = Sha256::new();
        hasher.update(self.seed.to_le_bytes());
        hasher.update((row_id.len() as u64).to_le_bytes());
        hasher.update(row_id.as_bytes());
        hasher.update(epoch.to_le_bytes());
        let digest = hasher.finalize();
        let mut u: u64 = 0;
        for &b in &digest[..8] {
            u = (u << 8) | b as u64;
        }
        (u as usize) % modulus
    }
}

// ─── Plan ───────────────────────────────────────────────────────────────

/// One emitted assignment in a training plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub row_id: String,
    pub epoch: u32,
    pub template_id: String,
}

/// Build a deterministic `(row, epoch) -> template` plan for
/// `rows × epochs`. Rows whose [`AvailableState`] satisfies no template
/// emit no entries (caller can detect these by comparing
/// `plan.len()` to `rows.len() * epochs`).
pub fn build_plan(
    library: &TemplateLibrary,
    rows: &[(String, AvailableState)],
    epochs: u32,
    seed: u64,
) -> Vec<PlanEntry> {
    let sampler = Sampler::new(library, seed);
    let mut plan = Vec::with_capacity(rows.len() * epochs as usize);
    for (row_id, available) in rows {
        for epoch in 0..epochs {
            if let Some(t) = sampler.pick(row_id, epoch, available) {
                plan.push(PlanEntry {
                    row_id: row_id.clone(),
                    epoch,
                    template_id: t.id.clone(),
                });
            }
        }
    }
    plan
}

/// Per-template count in `plan`. Useful for eval-time attribution and
/// for verifying the sampler distributes work across the satisfiable
/// subset rather than collapsing to one variant.
pub fn template_distribution(plan: &[PlanEntry]) -> std::collections::BTreeMap<String, usize> {
    let mut map = std::collections::BTreeMap::new();
    for entry in plan {
        *map.entry(entry.template_id.clone()).or_insert(0) += 1;
    }
    map
}

// ─── Library loader (thin wrapper for convenience) ──────────────────────

pub fn load_library_from_path(path: &std::path::Path) -> anyhow::Result<TemplateLibrary> {
    use anyhow::Context;
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading template library {}", path.display()))?;
    let lib: TemplateLibrary = serde_json::from_str(&text)
        .with_context(|| format!("parsing template library {}", path.display()))?;
    Ok(lib)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use refineforge_repair_api::proof_graph::{OutputFormat, PromptTemplate};

    fn mk_template(id: &str, requires: TemplateRequirements) -> PromptTemplate {
        PromptTemplate {
            id: id.into(),
            variant_name: id.into(),
            requires,
            user_template: format!("template-{id}"),
            system_prompt: None,
            expected_output_format: OutputFormat::SingleTactic,
        }
    }

    fn five_template_library() -> TemplateLibrary {
        TemplateLibrary {
            schema_version: 1,
            templates: vec![
                mk_template("fix_proof_direct", TemplateRequirements::default()),
                mk_template(
                    "goal_focused",
                    TemplateRequirements {
                        needs_goal: true,
                        ..Default::default()
                    },
                ),
                mk_template(
                    "goal_with_hypotheses",
                    TemplateRequirements {
                        needs_goal: true,
                        needs_hypotheses: true,
                        ..Default::default()
                    },
                ),
                mk_template(
                    "history_aware",
                    TemplateRequirements {
                        needs_goal: true,
                        needs_hypotheses: true,
                        needs_tactic_history: true,
                        ..Default::default()
                    },
                ),
                mk_template(
                    "graph_aware",
                    TemplateRequirements {
                        needs_goal: true,
                        needs_hypotheses: true,
                        needs_tactic_history: true,
                        needs_lemma_neighborhood: true,
                    },
                ),
            ],
        }
    }

    #[test]
    fn satisfies_checks_each_bit() {
        let s = AvailableState {
            has_goal: true,
            has_hypotheses: true,
            has_tactic_history: false,
            has_lemma_neighborhood: false,
        };
        assert!(s.satisfies(&TemplateRequirements::default()));
        assert!(s.satisfies(&TemplateRequirements {
            needs_goal: true,
            needs_hypotheses: true,
            ..Default::default()
        }));
        assert!(!s.satisfies(&TemplateRequirements {
            needs_tactic_history: true,
            ..Default::default()
        }));
    }

    #[test]
    fn sampler_is_deterministic_on_same_inputs() {
        let lib = five_template_library();
        let s = Sampler::new(&lib, 42);
        let avail = AvailableState::all_present();
        let a = s.pick("row-1", 0, &avail).unwrap().id.clone();
        let b = s.pick("row-1", 0, &avail).unwrap().id.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn sampler_varies_across_epochs() {
        let lib = five_template_library();
        let s = Sampler::new(&lib, 42);
        let avail = AvailableState::all_present();
        let mut seen = std::collections::BTreeSet::new();
        for epoch in 0..50 {
            seen.insert(s.pick("row-1", epoch, &avail).unwrap().id.clone());
        }
        // With 5 satisfiable templates and 50 epochs the sampler
        // should hit at least 4 distinct templates with hash-based
        // dispersion. (Strict 5/5 is statistically likely but not
        // guaranteed; we check >=4 to avoid flakes.)
        assert!(seen.len() >= 4, "expected diverse sampling, got {seen:?}");
    }

    #[test]
    fn sampler_filters_by_available_state() {
        let lib = five_template_library();
        let s = Sampler::new(&lib, 42);
        let goal_only = AvailableState {
            has_goal: true,
            ..Default::default()
        };
        // Only fix_proof_direct (no requirements) and goal_focused
        // (needs_goal only) are satisfiable.
        let allowed: std::collections::BTreeSet<&str> =
            ["fix_proof_direct", "goal_focused"].into_iter().collect();
        for epoch in 0..20 {
            let id = &s.pick("row", epoch, &goal_only).unwrap().id;
            assert!(allowed.contains(id.as_str()), "unexpected template {id}");
        }
    }

    #[test]
    fn sampler_returns_none_when_nothing_satisfiable() {
        let lib = TemplateLibrary {
            schema_version: 1,
            templates: vec![mk_template(
                "needs_everything",
                TemplateRequirements {
                    needs_goal: true,
                    needs_hypotheses: true,
                    needs_tactic_history: true,
                    needs_lemma_neighborhood: true,
                },
            )],
        };
        let s = Sampler::new(&lib, 0);
        assert!(s.pick("row", 0, &AvailableState::default()).is_none());
    }

    #[test]
    fn build_plan_emits_one_entry_per_satisfiable_row_epoch() {
        let lib = five_template_library();
        let rows = vec![
            ("r1".into(), AvailableState::all_present()),
            ("r2".into(), AvailableState::default()), // fix_proof_direct only
            (
                "r3".into(),
                AvailableState {
                    has_goal: true,
                    ..Default::default()
                },
            ),
        ];
        let plan = build_plan(&lib, &rows, 3, 42);
        assert_eq!(plan.len(), 3 * 3);
        let dist = template_distribution(&plan);
        // r2 always picks fix_proof_direct (only satisfiable template).
        assert!(*dist.get("fix_proof_direct").unwrap_or(&0) >= 3);
    }

    #[test]
    fn build_plan_is_byte_reproducible_for_same_seed() {
        let lib = five_template_library();
        let rows = vec![
            ("a".into(), AvailableState::all_present()),
            ("b".into(), AvailableState::all_present()),
        ];
        let p1 = build_plan(&lib, &rows, 5, 7);
        let p2 = build_plan(&lib, &rows, 5, 7);
        assert_eq!(p1, p2);
    }

    #[test]
    fn different_seeds_change_assignments() {
        let lib = five_template_library();
        let rows = vec![("a".into(), AvailableState::all_present())];
        let p1 = build_plan(&lib, &rows, 30, 1);
        let p2 = build_plan(&lib, &rows, 30, 2);
        // At least one assignment should differ across 30 epochs with
        // 5 satisfiable templates and two distinct seeds.
        assert_ne!(p1, p2);
    }

    #[test]
    fn template_distribution_counts_per_id() {
        let plan = vec![
            PlanEntry {
                row_id: "r1".into(),
                epoch: 0,
                template_id: "a".into(),
            },
            PlanEntry {
                row_id: "r1".into(),
                epoch: 1,
                template_id: "b".into(),
            },
            PlanEntry {
                row_id: "r2".into(),
                epoch: 0,
                template_id: "a".into(),
            },
        ];
        let d = template_distribution(&plan);
        assert_eq!(d["a"], 2);
        assert_eq!(d["b"], 1);
    }
}
