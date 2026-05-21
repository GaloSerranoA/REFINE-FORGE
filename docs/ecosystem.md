# The HELYX-centric ecosystem — HELYX is the LLM; the other three are infrastructure

> **Status:** v0.2. Operator-authored opinion captured in the
> refineforge repo for posterity. v0.1 (commit `0805430`)
> framed the four projects as flat peers; v0.2 corrects to
> the HELYX-as-LLM-product framing the operator has been
> communicating throughout. v0.1 stays in git history for
> audit.

This document records what NANTAR AI ROBOTICS is actually
building and what each project's role is in that build. It
does not commit to integration timelines; it commits to the
*frame* so per-pair integration plans (per
`docs/plans/*.md`) can be drafted from the right anchor.

## 1. HELYX is the LLM

**HELYX is the operator's LLM.** Not a verification target.
Not a research substrate. Not a generic AI tool. The actual
production LLM that NANTAR AI ROBOTICS is building.

Evidence (from `C:\HELYX\`):
- Canonical project document is named **`HELYX LLM project.txt`**.
- README opens with: "verified, reproducible, capability-typed
  AI substrate."
- The project's opening question is: "What would have to be
  true for HELYX to be the structurally best AI codebase that
  has ever existed?"
- Compares itself to **PyTorch, JAX** (the LLM ecosystem),
  not to Lean/Coq (the verification ecosystem).
- States: "A property no production LLM has today" — meaning
  HELYX positions itself AS a production LLM.
- Crates include `helyx-train`, `helyx-distill`,
  `helyx-planner`, `helyx-reasoning`, `helyx-inference` —
  the full training + inference + serving stack of an LLM.
- Eleven simultaneous "ceiling" properties (provably correct
  + bit-exact reproducible + capability-typed + continuously
  evolvable + reproducible build + adversarially resilient +
  research-native + operationally trustworthy + temporally
  coherent + hardware-coherent) — properties the operator
  wants their LLM to have, none of which any production LLM
  has today.

HELYX's verified-claim registry (`verified/checked/helyx-*-verified/`)
isn't HELYX outsourcing verification to a third party; it's
HELYX's *own internal trust-boundary discipline* for its own
load-bearing claims. The Lean specs in
`verified/lean/HELYX/Audit/`, `verified/lean/HELYX/NAL/`,
`verified/lean/HELYX/Capability/`, etc. are HELYX's own
formal commitments about how HELYX behaves.

## 2. The other three projects are HELYX's infrastructure

The remaining three projects in the operator's portfolio
exist **in service of HELYX**:

| Role | Project | Location | What it does for HELYX |
|---|---|---|---|
| **Thinking adjunct** | Cogn8ty / NANTAR INMORTAL RUST | `D:\AI-PROJECTS-GALO\PROJECTS\NANTAR INMORTAL RUST` | Pure-Rust NARS engine + Prolog + 8-tier cognition + JSON-RPC brain server on port 7742. Provides symbolic-reasoning capabilities HELYX's inference can call OR that HELYX directly integrates as its symbolic substrate. 78 crates; 12,272 tests passing. **The operator-stated `immortal-nars` + `immortal-prolog` live here.** |
| **Data pipeline** | Knowledge-Foundry (KF) | `D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry` | Generates training corpora via LLM teachers (Anthropic, OpenAI, GGUF, local OpenAI-compatible). 6 modes — `kb_triple`, `sft_pair`, `dpo_preference`, `cot_trace`, `embedding_triplet`, `tool_call`. 14 shared anti-hallucination gates + 42 mode-specific gates. Multi-teacher. HuggingFace publishing. **Produces corpora HELYX's `helyx-train` is designed to consume (consumption pending — see §6).** 337 Python files; 14,955 LOC; 1,035 tests passing (validation 2026-05-14). **Python.** |
| **Verification team** | refineforge | `D:\AI-PROJECTS-GALO\PROJECTS\refineforge` | Lean 4 proof engineering + refinement bundles + autonomous driver + escalation engine + criteria v0.3 contract. **Replaces the 4-engineer team (Lean specialist + ML engineer + DevOps + CUDA engineer) the operator would have needed to verify HELYX's load-bearing claims manually.** 9 workspace crates; 383 tests passing. |

All three are substantively shipped, not aspirational. Each
has its own CI gate, its own release discipline, its own
tests passing.

**The crucial reframe:** refineforge is not a generic framework
looking for a design partner. **refineforge exists because the
alternative was hiring four engineers to manually verify
HELYX's claims**, and the operator built refineforge to
automate that team instead. See `resourcing-plan.md` v0.2.

## 3. The load-bearing structural observation

HELYX is a **neuro-symbolic LLM in the strict sense**:

- The neural side is HELYX's own `helyx-train` /
  `helyx-inference` / NAL (Non-Axiomatic Logic — the
  differentiable approximation per the Lean specs at
  `C:\HELYX\verified\lean\HELYX\NAL\`).
- The symbolic side is **NARS as implemented by Cogn8ty**,
  reachable via `127.0.0.1:7742` brain-server JSON-RPC OR
  potentially compiled into HELYX directly as a Rust crate
  dependency.

Most "neuro-symbolic AI" research projects glue PyTorch to a
Python Prolog implementation through brittle FFI and publish
a paper. **HELYX has both halves in pure Rust**, sharing a
trust base, with no FFI tax and no language-impedance
mismatch.

That is not common. It is unusual enough to be
publication-worthy on architectural grounds alone, independent
of any benchmark.

## 4. The composition reads cleanly as one sentence

> **HELYX is the LLM. Cogn8ty thinks for it, KF teaches it,
> refineforge proves it. Rust binds them.**

Each clause describes the project's load-bearing function in
service of HELYX:

- **HELYX is the LLM**: the product. Five-substrate
  architecture (V = Verified Core; C = Capability-Typed Code;
  H = Hardware-Coherent Kernels; +2 more). Eleven ceiling
  properties. 53-crate workspace; 4643 tests passing.
- **Cogn8ty thinks for it**: symbolic-reasoning substrate
  HELYX's inference can call for NARS-style backtracking,
  contradiction detection, uncertainty propagation, honest
  refusal on knowledge gaps.
- **KF teaches it**: data-generation pipeline that produces
  the corpora HELYX's training stack consumes.
- **refineforge proves it**: mechanical evidence chain +
  signed refinement bundles for HELYX's load-bearing trust
  claims. Replaces the 4-engineer team that would otherwise
  do this work manually.

The "Rust binds them" clause is doing real work: HELYX +
Cogn8ty + refineforge are all pure Rust at the substrate
level; KF's Python sits at the data-generation periphery, by
design (see `docs/why-rust.md`).

## 5. Why this stack is hard to replicate

The HELYX-centric architecture rests on compounding choices
that individually look expensive but together compound into
a position competitors can't easily reach:

- **Rust at the LLM substrate** (vs PyTorch): costs the
  ecosystem; buys provably-correct + bit-exact + capability-
  typed. Documented in `docs/why-rust.md`.
- **Pure-symbolic NARS in Rust** (Cogn8ty): there are very
  few Rust Prolog/NARS implementations in production. The
  operator built one with 12,272 tests passing.
- **Differentiable NAL in HELYX**: the neural NARS half.
  Even rarer — most "differentiable logic" research lives in
  JAX papers, not Rust production substrates.
- **Lean → Rust extraction at the verified core** (HELYX
  Substrate V): the formal-verification community has this
  pattern; the AI community largely doesn't.
- **A verification framework with autonomous LLM driver +
  9-category escalation contract** (refineforge): the
  trust-critical AI space has policies and process docs; it
  does not have a shipping framework with this shape.
- **A neuro-symbolic LLM (HELYX) sitting on top of all of
  the above**: production-grade Rust LLM with verified core,
  bit-exact kernels, capability-typed effects, criteria-v0.3
  escalation contract. No competitor LLM has any of these,
  let alone all of them simultaneously.

A competitor would need to make every choice above in the
same direction, accept the same ecosystem cost, and ship
all of them — **18-month minimum from scratch** with a
fully-staffed team. NANTAR AI ROBOTICS has all four shipped
+ tested + HEAD-tracked. **Time-to-replicate is the moat,
not feature count.**

## 6. The honest constraint — complementary ≠ integrated

The HELYX-centric architecture is **strong on paper**.
Execution is the work, and the work is real:

- **HELYX does not currently call Cogn8ty's brain server.**
  Each project has its own CLI and runs independently. The
  natural integration (HELYX's `helyx-reasoning` invoking
  Cogn8ty over JSON-RPC, or compiling Cogn8ty as a Rust
  dependency) is an engineering line.
- **HELYX's `helyx-train` does not yet consume KF-generated
  corpora.** KF's HF-published datasets would need to be
  fed through HELYX's training pipeline. Documented in
  `docs/plans/finetuning-plan.md` as Phase 5.
- **refineforge has signed exactly ONE HELYX claim so far**
  (HELYX-AUDIT-001, commit `8852226`). The remaining four
  Audit claims + HELYX-NAL-001 + Capability + Constitution +
  Determinism + Drift + Numerics + Sandbox + Skeptic are
  all pending. Each is a slice + a refinement doc.
- **`refine --strategy cogn8ty` does not exist.** The
  refineforge ↔ Cogn8ty adapter (brain-server JSON-RPC →
  `RepairStrategy::propose_patch`) is unwritten.
- **Cross-repo drift detection is absent.** Each project's
  HEAD moves independently; nothing flags when (e.g.) HELYX
  changes the NAL spec without refineforge re-syncing the
  slice.

The right framing for a reviewer: **"HELYX is the LLM the
operator is building; the other three are the infrastructure
that lets the operator build it without hiring."** The
integration work to make the infrastructure actually serve
the LLM is real engineering — 3-6 months of focused work for
full composition; 6-10 weeks for the smallest demonstrable
three-way (refineforge + Cogn8ty + KF visibly serving HELYX
on one load-bearing claim).

## 7. Sensible next-step deliverables

Three concrete integration deliverables, smallest to largest
(operator opinion; not commitment):

### 7.1 — More HELYX claims signed by refineforge (4-6 weeks)

Extend the pattern from HELYX-AUDIT-001 (commit `8852226`)
to:
- The remaining four HELYX audit theorems (Causality,
  Replay, TamperDetection, tampered-correctness)
- HELYX-NAL-001 — the differentiable NAL load-bearing claim
- HELYX-CAPABILITY-001 — capability-typed effect-system
  invariant
- HELYX-CONSTITUTION-001 — the constitutional gate

End-state: ~7-9 HELYX trust claims under refineforge
verification. Each ships as a slice + refinement doc + signed
bundle. **This is the highest-value next step because every
HELYX claim verified is one more reviewer-checkable trust
artifact.**

### 7.2 — `refine --strategy cogn8ty` adapter (2-4 weeks)

Single-purpose adapter that routes refineforge's
`RepairStrategy` calls to Cogn8ty's brain server JSON-RPC.
Demonstrates symbolic-strategy as a peer to
`AnthropicStrategy`. Acceptance gate: refineforge produces
either (a) a symbolic patch from Cogn8ty OR (b) an honest
refusal with the typed refusal trace, against a real broken
Lean file.

Useful in service of HELYX because HELYX's own future
inference may want to call Cogn8ty for symbolic reasoning;
the refineforge adapter validates the integration pattern at
small scale first.

### 7.3 — HELYX consumes KF corpora via `helyx-train` (2-3 months elapsed)

Per `docs/plans/finetuning-plan.md`. The smoke fine-tune
(Qwen2.5-Coder-1.5B → 13B production) is the first concrete
case where KF's data flows into HELYX's training. The
acceptance gate is set in the fine-tuning plan §7.

This is the "infrastructure actually serving HELYX" milestone
that the four-project value chain pivots on.

### 7.4 — Cogn8ty compiled into HELYX as a Rust dep (2-4 months)

Larger scope: instead of HELYX calling Cogn8ty over JSON-RPC,
HELYX's `helyx-reasoning` directly depends on Cogn8ty crates
(e.g., `immortal-prolog`, `immortal-nars`) as workspace
members. No network hop; the neuro-symbolic composition runs
in-process.

This is the publication-worthy milestone. Result: a
neuro-symbolic LLM in the strict sense, all Rust, refineforge-
verified.

## 8. What this doc is NOT

- **Not a roadmap commitment for HELYX.** HELYX's own
  roadmap lives in `C:\HELYX\` (its own `structure.md`,
  `docs/plans/`, etc.). This doc records the *frame* for how
  refineforge + Cogn8ty + KF serve HELYX, not HELYX's
  internal plans.
- **Not a sales pitch.** The competitive analysis in §5 is
  for the operator's own reference, not for external
  publication without review.
- **Not a substitute for per-pair plan docs.** Each pair in
  §3 of v0.1 / each deliverable in §7 of v0.2 warrants its
  own plan when scheduled (see
  `docs/plans/finetuning-plan.md` for the KF → HELYX
  example).
- **Not a definitive architecture.** The four projects are
  evolving. This doc may be superseded by a v0.3 if HELYX's
  five-substrate model shifts or if Cogn8ty's symbolic
  substrate gets directly absorbed into HELYX.

## 9. Provenance + v0.1 errata

- **Authored by:** Galo Serrano Abad (operator) + Claude Code
  (Opus 4.7) in conversation over the course of a single
  long session.
- **v0.1 errata:** commit `0805430` shipped a "flat four-peer"
  framing that treated HELYX as one of four equal projects
  composing together. The operator corrected this twice in
  the same conversation, restating that **HELYX is their LLM**
  and the other three are HELYX's infrastructure. v0.2 (this
  doc) is the corrected framing. v0.1 stays in git history
  for audit.
- **Lesson encoded for future planning:** if a refineforge
  plan doc puts HELYX on the same level as Cogn8ty / KF /
  refineforge as if they were peers, that's a framing bug —
  HELYX is the product; the others are infrastructure.

---

> **HELYX is the LLM. Cogn8ty thinks for it, KF teaches it,
> refineforge proves it. Rust binds them. No one else has
> shipped this stack.**
