# The four-project ecosystem — Cogn8ty + HELYX + KF + refineforge

> **Status:** Operator-authored opinion captured in the
> refineforge repo for posterity. Survives the conversation
> that produced it. v0.1.

This document records what the four projects in NANTAR AI
ROBOTICS's portfolio actually are, how they structurally
complement each other, and why the *combination* is the
position no competitor can replicate quickly. It does not
commit to integration timelines; it commits to the
*conceptual map* so that integration plans (per project pair)
can be drafted from a shared frame.

## 1. The four projects, mapped to structural layers

| Layer | Project | Location | What it owns |
|---|---|---|---|
| **Data** | Knowledge-Foundry (KF) | `D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry` | Generates training corpora via LLM teachers. 5 modes (sft_pair / dpo_preference / cot_trace / embedding_triplet / tool_call). Multi-teacher. HuggingFace publishing. 330 source files; 1115 tests passing. **Python.** |
| **Substrate (symbolic)** | Cogn8ty / NANTAR INMORTAL RUST | `D:\AI-PROJECTS-GALO\PROJECTS\NANTAR INMORTAL RUST` | Pure-Rust NARS engine + Prolog (SLD + WAM) + 8-tier cognition pipeline + JSON-RPC brain server on port 7742. 78-crate workspace; 12,272 tests passing. Refusal taxonomy, contradiction detection, uncertainty propagation. **Honesty-by-construction.** |
| **Substrate (neural)** | HELYX + NAL | `C:\HELYX` | Five-substrate AI architecture: V (Verified Core via Lean 4 + Rust extraction) + C (Capability-typed Rust) + H (Hardware-coherent CUDA/Metal kernels) + two more. NAL = differentiable Non-Axiomatic Logic. 53-crate workspace; 4643 tests passing. **Eleven simultaneous "ceiling" properties.** |
| **Trust** | refineforge | `D:\AI-PROJECTS-GALO\PROJECTS\refineforge` | Lean 4 proof engineering + refinement bundles + autonomous driver + escalation engine. 9 workspace crates; 383 tests passing. Criteria v0.3 contract. First HELYX claim signed in commit `8852226`. |

All four are **substantively shipped, not aspirational.**
Each has its own CI gate, its own test suite passing, its
own release discipline. None is vapor.

## 2. The load-bearing structural observation

**Cogn8ty (symbolic NARS) and HELYX-NAL (differentiable NARS)
are the two halves of a NARS system in the strict sense Pei
Wang defined.**

Most "neuro-symbolic" projects in the literature glue PyTorch
to a Python Prolog implementation through brittle FFI and
publish a paper. NANTAR AI ROBOTICS has built **both halves
in pure Rust, sharing a trust base**, with no FFI tax and no
language-impedance mismatch. That is not common; it is
unusual enough to be publication-worthy on architectural
grounds alone, independent of any benchmark.

The structural consequence: a NARS-based reasoner that
combines Cogn8ty's symbolic inference + HELYX-NAL's
differentiable approximation **could be deployed as a single
Rust binary** with capability-typed effects, bit-exact
reproducibility, and a Lean-extracted verified core anchoring
its trust properties. No competitor stack (PyTorch + a Python
NARS port + a separate Rust audit layer) approaches this
shape.

## 3. Pair-by-pair complementarity matrix

The four projects compose pairwise. Each pair has a concrete
"what one gives the other" relationship:

| Pair | What flows | Direction |
|---|---|---|
| KF → HELYX | Training corpora (HuggingFace datasets) | KF feeds HELYX-NAL's training pipeline |
| KF → Cogn8ty | NARS-shaped reasoning traces; Prolog clauses | KF could distill symbolic data, though current modes are LLM-shaped |
| KF → refineforge | Eval corpus for proof repair | Per `docs/plans/finetuning-plan.md` §10 probe set |
| HELYX-NAL ↔ Cogn8ty | The neural and symbolic halves of NARS | Bidirectional; sketched but not committed |
| HELYX → refineforge | Production claims to verify | refineforge ships HELYX-AUDIT-001 in commit `8852226`; next: HELYX-NAL-001 + the Audit/* sequel theorems |
| Cogn8ty → refineforge | Symbolic-reasoning `--strategy cogn8ty` adapter | brain_reason JSON-RPC → RepairStrategy::propose_patch; refusal/uncertainty maps to honest decline |
| refineforge → Cogn8ty | Signed bundles asserting Cogn8ty's reasoning soundness | E.g., Prolog WAM correctness, NARS revision rule soundness |
| refineforge → KF | Signed bundles asserting corpus-quality claims | Provenance + license + quality-gate evidence preserved through refineforge's bundle exporter |

The composition is **not aspirational** — each pair already
has a concrete, sub-month integration path the operator could
execute. What's missing is the *engineering time* to wire
them.

## 4. The composition reads cleanly as one sentence

> **Cogn8ty thinks, HELYX runs, KF teaches, refineforge proves.
> Rust binds them.**

Each verb describes the project's load-bearing function:
- **Cogn8ty thinks**: symbolic reasoning + uncertainty
  propagation + refusal taxonomy.
- **HELYX runs**: production AI substrate with capability
  types + verified core + bit-exact kernels.
- **KF teaches**: data-generation pipeline that feeds the
  learning loops.
- **refineforge proves**: mechanical evidence + signed
  refinement bundles that audit the rest.

The "Rust binds them" clause is doing real work: the entire
stack lives in one language at the substrate (KF's Python
sits at the data-generation periphery, by design — see
`docs/why-rust.md`). No FFI, no language-impedance, one
trust base, one `cargo test` discipline.

## 5. The honest constraint — complementary ≠ integrated

The four-way value proposition is **strong on paper**.
Execution is the work, and the work is real:

- **None of the four projects currently calls into another
  via a shared API.** Each has its own CLI, its own release
  artifacts, its own commit machine.
- **`refine --strategy cogn8ty` does not exist.** The brain
  server boots on port 7742; the adapter that maps
  `brain_reason` to `RepairStrategy::propose_patch` is
  unwritten. 2-4 weeks of engineering.
- **KF's Lean probe set is speced but not authored.** See
  `docs/plans/finetuning-plan.md` §10. 3-5 days of
  Knowledge-Foundry maintainer time when Phase 1 starts.
- **HELYX-NAL-001 is not yet refineforge-bridged.** Mirror
  of HELYX-AUDIT-001 from commit `8852226`; similar slice
  + refinement doc + bundle. ~1-2 hours.
- **Cogn8ty ↔ HELYX-NAL composition** is sketched but not
  committed. Real engineering line (likely months).
- **Cross-repo CI / drift detection** is absent. Each
  project's HEAD moves independently; nothing flags when
  Cogn8ty's NARS revision rule diverges from HELYX-NAL's
  differentiable approximation.

The right framing for a reviewer: **"these four projects can
compose; they don't yet."** The bet is that the architectural
choices already made (pure Rust at the substrate; criteria
v0.3 contract; Lean-extracted verified core; capability-typed
effects) mean integration is a tractable engineering
program, not a research program. Integration timeline
estimate: **3-6 months of focused work** to ship the full
four-way composition; **6-10 weeks** to ship the smallest
demonstrable three-way (refineforge + Cogn8ty + KF) with a
single load-bearing artifact.

## 6. Why this position is hard to replicate

The four-way bet rests on architectural choices that
individually look uneconomic but together compound:

- **Rust at the substrate** (vs PyTorch): costs the
  ecosystem; buys provably-correct + bit-exact + capability-
  typed. Documented in `docs/why-rust.md`.
- **Pure-symbolic NARS in Rust** (Cogn8ty): there are very
  few Rust Prolog/NARS implementations in production. The
  operator built one.
- **Differentiable NAL in HELYX** (the neural NARS half):
  even rarer — most "differentiable logic" research lives
  in JAX papers, not Rust production substrates.
- **Lean → Rust extraction** (HELYX Substrate V): the
  formal-verification community has this pattern; the AI
  community largely doesn't.
- **Verification framework with autonomous LLM driver +
  9-category escalation contract** (refineforge): the trust-
  critical AI space has policies and process docs; it does
  not have a shipping framework with this shape.

A competitor would need to:
1. Pick Rust for the substrate (costs ecosystem; benefits
   long-term).
2. Build a Rust NARS (rare expertise; 12,272 tests aren't
   trivial).
3. Build a Rust differentiable logic (research-grade).
4. Build a Lean extraction pipeline + audit verified-claim
   registry.
5. Build a verification framework + criteria contract.
6. *Then* compose them.

Each is a multi-quarter commitment. The four together are an
18-month minimum starting from scratch with a fully-staffed
team. NANTAR AI ROBOTICS has all four shipped, tested, and
HEAD-tracked as of this commit. **Time-to-replicate is the
moat, not feature count.**

## 7. Strategic recommendation (this section is operator opinion, not commitment)

Three concrete integration deliverables, smallest to largest:

### 7.1 — `refine --strategy cogn8ty` adapter (2-4 weeks)

Single-purpose: route refineforge's `RepairStrategy` calls
to Cogn8ty's brain server JSON-RPC. Map `brain_reason`
response → `Patch | None` (with the honest "None on
knowledge_gap=true" mapping). Acceptance gate: a real broken
Lean file gets either (a) a symbolic patch from Cogn8ty's
Prolog/NARS chain OR (b) an honest refusal. Demonstrates the
symbolic-strategy path end-to-end.

### 7.2 — HELYX-NAL-001 + supporting Audit/* claims (4-6 weeks)

Mirror the HELYX-AUDIT-001 pattern (commit `8852226`) for
the differentiable-NAL claim AND the remaining four audit
theorems (Causality, Replay, TamperDetection,
tampered-correctness). Each gets a refinement doc + signed
bundle. End-state: 6+ HELYX claims under refineforge
verification.

### 7.3 — Cogn8ty ↔ HELYX-NAL composition (2-4 months)

The big one. Connect Cogn8ty's symbolic inference layer to
HELYX-NAL's differentiable approximation through a shared
TruthValue / confidence representation. Refineforge verifies
the composition's load-bearing invariants (e.g., that the
symbolic revision rule and the differentiable approximation
agree on a finite test grid; that refusal signals propagate
bidirectionally). Result: a publishable neuro-symbolic
reasoning system whose trust artifacts are
refineforge-signed.

These are not committed deliverables. They are the **three
sensible next steps** the operator could pick from. Picking
one is a separate operator decision; this doc records that
they exist.

## 8. What this doc is NOT

- **Not a roadmap commitment.** Recording strategic context;
  not signing up for any timeline.
- **Not a sales pitch.** The competitive analysis in §6 is
  for the operator's own reference, not for external
  publication without review.
- **Not a substitute for the per-project plans.** Each pair
  in §3 warrants its own plan doc (see
  `docs/plans/finetuning-plan.md` for the KF → refineforge
  example; `docs/plans/gui-plan.md` for refineforge GUI;
  none yet for Cogn8ty integration).
- **Not a definitive architecture.** The four projects are
  evolving; this doc may be supersededby a v0.2 once one of
  the integration deliverables in §7 ships.

## 9. Provenance

- **Authored by:** Galo Serrano Abad (operator) + Claude Code
  (Opus 4.7) in conversation.
- **Triggered by:** the operator pointing the assistant at
  three external verification directories on Desktop
  (`helyx-verification`, `verification-2026-05-16` for KF,
  `cogn8ty-verification`) over the course of one session,
  revealing the four-project structure that previous
  refineforge docs had treated as out-of-scope.
- **Preserved here because:** the operator wanted the
  opinion to survive the conversation. Future readers
  (including future Claude sessions) should treat this as
  the canonical operator-frame for thinking about the
  ecosystem.

---

> **Cogn8ty thinks, HELYX runs, KF teaches, refineforge proves.
> Rust binds them. No one else has shipped all four.**
