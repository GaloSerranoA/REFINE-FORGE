# `training/prompt_templates/`

Multi-prompt instruction-tuning templates for Lean 4 proof repair.

**Inspired by** [InstructGLM (Ye et al., EACL 2024, _"Language is All a Graph
Needs"_)](https://arxiv.org/abs/2308.07134): the same proof state is rendered
through multiple natural-language prompts, and the fine-tuner samples a
template per example each epoch. This produces ~N× the effective coverage on
prompt-variation without growing the corpus and prevents overfitting to a
single phrasing.

## File layout

| File | Purpose |
|---|---|
| `lean_proof_repair_v1.json` | v1 library, 5 templates. Consumed by `refineforge-trainer` and by `refine repair` strategies that want richer prompt context. |
| `README.md` | This file. |

## Schema

Defined by `crates/refineforge-repair-api/src/proof_graph.rs`:

- `TemplateLibrary { schema_version: u32, templates: Vec<PromptTemplate> }`
- `PromptTemplate { id, variant_name, requires, user_template, system_prompt?, expected_output_format }`
- `TemplateRequirements { needs_goal, needs_hypotheses, needs_tactic_history, needs_lemma_neighborhood }` — bitset; renderer fails fast (`RenderError::MissingField`) if a required field is empty in the `ProofState`.
- `OutputFormat`: `free_form` | `single_tactic` | `patch_json` | `verifier_verdict`.

## Placeholder grammar

The renderer (`proof_graph::render`) substitutes these placeholders in
`user_template`:

| Placeholder | Source |
|---|---|
| `{goal}` | `ProofState.current_goal`, or `"(unknown)"` if empty |
| `{hypotheses}` | newline-joined `name : type` lines |
| `{tactic_history}` | newline-joined `L<line>: <tactic>` lines |
| `{lemmas}` | newline-joined `<distance>-hop  <name>  :  <signature>` |
| `{diagnostic_line}` | diagnostic anchor line number |
| `{diagnostic_message}` | diagnostic message text |
| `{diagnostic_severity}` | `error` / `warning` / `information` / `hint` / `unknown` |

Unknown placeholders raise `RenderError::UnknownPlaceholder`.
Literal braces are escaped with `{{` and `}}`, so templates can show JSON
examples without being interpreted as placeholders.

## Templates in v1

| id | requires | output | use case |
|---|---|---|---|
| `fix_proof_direct` | none | `patch_json` | minimal baseline — diagnostic only, no proof state |
| `goal_focused` | `needs_goal` | `single_tactic` | one-tactic answer when only the goal is known |
| `goal_with_hypotheses` | + `needs_hypotheses` | `single_tactic` | hypothesis-aware tactic selection |
| `history_aware` | + `needs_tactic_history` | `single_tactic` | avoids repeating tried tactics |
| `graph_aware` | + `needs_lemma_neighborhood` | `single_tactic` | InstructGLM-style: full lemma-graph context (the lift) |

## Sampling policy (recommended for `refineforge-trainer`)

1. Determine which templates the current extractor can satisfy (its
   `ProofState` is rich enough for the template's `requires`).
2. Within that satisfiable subset, sample uniformly across templates each
   epoch per training example.
3. Log the template id with each emitted (prompt, target) pair so the run can
   compute per-template attribution at eval time.

A simpler v0 sampler that picks one template per epoch globally is acceptable
when the corpus is small.

## Versioning

`schema_version` is bumped when the placeholder grammar or the
`TemplateRequirements` fields change. New templates can be added without
bumping the version. Removing or renaming a template id is a breaking change
and requires a new file (`lean_proof_repair_v2.json`).

## Honest scope

- The `ProofState` extractor that fills `hypotheses`, `tactic_history`, and
  `lemma_neighborhood` is **stubbed** in `proof_graph::DiagnosticOnlyExtractor`
  (anchor only). Real extraction requires LSP integration (Lean's
  `$/lean/plainGoal`) and a Mathlib lemma index — out of scope for this drop,
  on the roadmap for the Lean specialist (see `ARCHITECTURE.md` Section 1).
- Until the richer extractor lands, templates beyond `fix_proof_direct` and
  `goal_focused` will not fully render. The schema is shipped now so the
  training corpus shape can stabilize alongside the extractor work.
