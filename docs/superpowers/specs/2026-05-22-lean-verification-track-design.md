# Lean Verification Track Design

Date: 2026-05-22
Status: Design approved in chat; awaiting review of this saved spec before implementation planning.

## Scope

This spec covers Part 1 of the four-part refineforge development sequence:
the Lean 4 / verification engineer track. It deliberately excludes the ML
fine-tuning track, release/DevOps track, and GPU/kernel track except where
their interfaces depend on the verification core.

The track has two stages:

1. Harden the existing verification core.
2. Dogfood the hardened core on a production-shaped claim.

## Current Context

The live repository already contains the main verification foundation:

- Lean project under `lean/`.
- Claim registry under `claims/`.
- Claim loader, no-sorry gate, Lean runner, bundle exporter/verifier,
  scaffolding, and source scanner in `crates/refineforge-cli`.
- `#[derive(LeanModel)]` proc macro in `crates/refineforge-derive`.
- Tutorial claims and refinement docs for EXAMPLE-001 and EXAMPLE-002.
- Escalation criteria for trust-base changes, including Mathlib first use.

The repo also has known verification-core gaps:

- `ARCHITECTURE.md` still opens with "Three Sections" even though Section 4
  now exists.
- `scan` is still a source text name-presence check rather than a structured
  Rust symbol check.
- Mathlib-aware bundle handling is documented as open work.
- The derive macro has tests, but its generated Lean contract and limitations
  can be made clearer.
- The template set can better exercise state-machine, capability, and
  refinement-obligation patterns.

## Goals

- Make the verification core harder to misuse.
- Make docs and ownership boundaries match the current four-section system.
- Improve scanner fidelity without breaking existing claim YAMLs.
- Make scan and lint outputs deterministic enough for bundle and review use.
- Preserve the stable bundle schema unless a concrete need for schema version
  2 appears.
- Preserve the existing `RepairStrategy` trait boundary; this track should not
  change ML strategy APIs.
- Add focused tests for each hardening change.
- Validate the result with a real claim path that includes Lean, Rust source,
  a refinement argument, scan, bundle export, and bundle verification.

## Non-Goals

- No real model training or checkpoint loading.
- No CUDA kernel implementation.
- No release signing redesign.
- No broad rewrite of the autonomous driver.
- No claim that Rust binaries are formally verified. The trust-critical
  artifact remains the human-reviewed refinement argument.

## Stage 1: Verification Core Hardening

### 1. Documentation Alignment

Update the high-level docs so they consistently describe four sections:

- `ARCHITECTURE.md`: title, opening table, sequencing, and timeline.
- `ROLES.md`: sequencing and mapping table.
- `STRUCTURE.md` if it still describes stale ownership boundaries.
- `README.md` only if the component status table is contradicted elsewhere.

The result should make clear that the Lean track owns the proof core and stable
interfaces, while Section 4 owns kernel bit-exact gates.

### 2. Structured Rust Source Scan

Upgrade `refine scan` from best-effort name search toward structured parsing:

- Prefer `syn` parsing for Rust files when possible.
- Detect top-level structs, enums, traits, impl methods, free functions, and
  associated functions that can be checked against `claims/*.yaml`.
- Keep a conservative fallback for files that cannot be parsed, with explicit
  warning text.
- Avoid false success from identifiers that appear only in comments or string
  literals.
- Emit discovered symbols in deterministic order and compute a deterministic
  scan-result hash when scan evidence is consumed by reports or bundles.

This should stay backward-compatible with the current claim schema.

### 3. Claim Linter

Add a small pre-Lean claim linter that catches human-maintenance errors before
the expensive or noisy gates run:

- Missing or unreadable Rust source citations.
- Missing refinement doc for claims marked as refined.
- Missing required refinement doc sections for production-shaped claims.
- Rust symbols cited by a claim but not discovered by structured scan.
- Discovered Rust symbols that are listed in a claim but no longer used by the
  refinement doc.
- Stale claim `status` values, especially `model+refined` without a complete
  refinement argument and scan evidence.

The linter should report warnings separately from hard errors. It should be
usable directly by the operator and callable from later bundle/autonomous
workflows without changing the claim schema.

### 4. Derive Macro Contract

Harden `#[derive(LeanModel)]` as a verification aid:

- Add tests for supported and rejected Rust field shapes.
- Clarify generated Lean output and limitations in docs.
- Ensure generated declarations are deterministic and stable across runs.
- Keep unsupported cases explicit rather than silently generating weak models.

The macro does not prove refinement. It generates a model skeleton that the
human refinement argument can cite.

### 5. Template and Claim Scaffolding

Strengthen templates that matter for production claims:

- State-machine invariants.
- Capability authorization.
- Capability with monotone revocation.
- Single-use token / linear-resource pattern if already present.

Each hardened template should scaffold:

- Lean module.
- Claim YAML.
- Refinement doc skeleton.
- Rust source citation when applicable.
- Template provenance metadata, such as a comment or metadata line recording
  the template name and template version used to generate the claim. This is
  not a claim-schema version bump.

The template path should remain usable through `refine new`.

### 6. Mathlib Trust-Base Handling

Clarify and test the first-use boundary for Mathlib and other Lake
dependencies:

- First Mathlib use remains a Category 8 trust-base expansion.
- The decision packet should include dependency name, version or commit pin,
  rationale, and expected proof surface.
- Bundle handling should either stay source-bundle-only with an explicit
  transit-of-trust statement, or add a narrow Mathlib metadata manifest without
  changing the bundle schema.

Stage 1 should not silently vendor Mathlib or claim third-party proof trust
without operator approval.

### 7. Regression Tests

Add targeted verification tests rather than broad refactors:

- Scanner tests for real Rust syntax and comment/string false positives.
- Scanner tests for deterministic symbol ordering and stable scan-result hash.
- Claim-linter tests for missing citations, missing refinement docs, unused or
  missing Rust symbols, and stale claim status.
- Derive macro tests for deterministic output and unsupported types.
- Template smoke tests where the repo already supports them, including
  template provenance metadata.
- No-sorry/no-admit/no-axiom regression tests if new scaffolds touch Lean.
- Bundle export/verify tests only where changed behavior affects manifests.

## Stage 2: Production-Claim Dogfood

After Stage 1 hardening, add one production-shaped claim inside this repo.

Default target if the operator does not name an external subsystem:

- A capability-with-revocation or state-machine claim under the existing
  template family, because it exercises proof structure, Rust source scanning,
  and a non-trivial refinement argument without cross-repo dependencies.

The claim must include:

- Lean model and theorem.
- Rust implementation or cited Rust source.
- Claim YAML with accurate status.
- Refinement argument document.
- `refine lean check <CLAIM>`.
- `refine scan check <CLAIM>`.
- `refine bundle export <CLAIM>`.
- `refine bundle verify artifacts/<CLAIM>`.

Any idealisation that the proof does not cover must be named in the
refinement doc. Any trust-base expansion must produce an operator decision
surface rather than being hidden in code.

## Interfaces

The Lean track owns these stable surfaces:

- Claim YAML schema.
- Bundle manifest schema.
- No-sorry policy behavior.
- Lean runner behavior.
- Scaffolding output contract.
- Rust source scan result semantics.
- `#[derive(LeanModel)]` generated model contract.

The Lean track must not break these without a documented schema/version change:

- Section 2 `RepairStrategy` integration.
- Section 3 bundle signing and verification assumptions.
- Section 4 bit-exact gate orchestration.

## Verification Plan

Run the narrow gates for the touched surfaces first:

- `cargo test -p refineforge-cli`
- `cargo test -p refineforge-derive`
- `cargo test -p example-counter`
- `cargo test -p refineforge-escalation` if trust-base packet behavior changes
- `(cd lean && lake build)` if Lean modules/templates change
- `cargo run --bin refine -- lean check-all`
- `cargo run --bin refine -- scan check-all`
- Bundle export and verify for the new dogfood claim

If broad workspace gates are run, report them separately from the narrow
acceptance gates.

## Acceptance Criteria

Stage 1 is complete when:

- Docs consistently describe four sections.
- Structured scan avoids comment/string false positives and passes existing
  claim checks.
- Scan results have deterministic ordering and stable hashing where consumed.
- A claim linter catches missing citations, missing refinement docs, stale
  status, and symbol drift before Lean runs.
- Derive macro behavior is documented and tested.
- At least one production-relevant template path is hardened and records
  template provenance metadata.
- Mathlib/trust-base handling is explicit and tested where code changes touch
  it.
- Relevant Rust and Lean gates pass.

Stage 2 is complete when:

- One production-shaped claim is added.
- The claim has Lean, Rust citation, claim YAML, and refinement doc.
- `lean check`, `scan`, `bundle export`, and `bundle verify` all pass for it.
- Any remaining limitations are documented as honest boundaries, not hidden
  implementation gaps.

## Risks

- Structured Rust scanning can become too ambitious. The first version should
  improve symbol detection without becoming a full compiler front-end.
- Mathlib bundling can explode scope. The first hardening pass should focus on
  explicit trust-base metadata and escalation, not vendoring all dependencies.
- A new production claim can turn into domain work. Keep the default claim
  repo-local unless the operator explicitly selects an external subsystem.
- Broad workspace gates can be slow or noisy. Use narrow gates as acceptance
  evidence and label any broader failures precisely.
