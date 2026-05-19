# `refine autonomous` — enterprise build plan

> **Status:** PLAN ONLY. No code in this commit. Plan is intended
> to be costed, scoped, and reviewed BEFORE any of phases 1-5 below
> are executed. The build is honestly estimated at **2 focused
> engineering weeks** (10 working days) for one experienced
> Rust + LLM-integration engineer.

## 1. Goal

Build the `refine autonomous <CLAIM-ID>` subcommand. It drives all
four sections of refineforge (Lean / ML / DevOps / CUDA) without
per-step human approval, escalating **only** when its proposed
next action matches one of the 8 categories in
[`escalation-criteria.md`](escalation-criteria.md).

**Success criterion (acceptance gate for the whole project):** on
EXAMPLE-002 with the Counter idealisation as bait, the autonomous
driver produces **exactly one** Category-2 (Idealisation)
escalation packet, waits for human approval, then produces a
sealed bundle with the operator's signature on the packet AND on
the bundle. End-to-end on a single developer machine in under 5
minutes wall-clock excluding human decision time.

## 2. Sequencing rationale

```
┌─────────────────────────────────────────────────────────────────┐
│  Phase 0  (DONE)  docs/escalation-criteria.md                   │
│           The contract. Audit + edit BEFORE any code enforces it.│
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 1  refineforge-escalation crate                          │
│           Pure engine — no AI, no I/O. Given a proposed action  │
│           struct, returns Escalate(category, reason) or Proceed.│
│           Unit-tested per category with canned scenarios.       │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 2  Decision-packet generator + git checkpoint            │
│           Template engine, evidence collectors, dissent slot.   │
│           Writes packet → escalations/<id>/, watches for human  │
│           signature commit, parses APPROVED / REJECTED.         │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 3  refineforge-autonomous driver                         │
│           Planner that wraps existing strategies. Calls         │
│           escalation engine before every step. Pauses on        │
│           Escalate, continues on Proceed. Final summary diff.   │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 4  EXAMPLE-002 dogfood + criteria v0.2                   │
│           Run on the deliberate idealisation. Refine criteria   │
│           based on what fires + what doesn't.                   │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 5  Docs + integration + release ritual                   │
│           Update README/STRUCTURE/CHANGELOG. New release.       │
└─────────────────────────────────────────────────────────────────┘
```

Why this order:
- **Criteria first** because they're the product. Code enforces a
  contract; the contract must be debatable in prose before it's
  enforceable in code.
- **Engine before driver** because the engine is small, pure-
  functional, and 100% testable. The driver is large and
  has external dependencies (LLM, git, lake). Get the
  decidable-piece right first.
- **Packet generator before driver** because the driver's value
  is only realised through packet quality. A driver writing bad
  packets is worse than no driver — the human starts rubber-
  stamping.
- **EXAMPLE-002 dogfood before any real claim** because the
  iteration cost on a synthetic claim is hours; on a real claim
  it's weeks.

## 3. Phase-by-phase

### Phase 0 — Criteria doc — ✅ shipped

- **Deliverable:** `docs/escalation-criteria.md` (this commit).
- **Time:** ½ day (done).
- **Definition of done:** human operator has read the 8 categories
  + open-question list and either approved as-is OR proposed
  edits that have been merged. Doc version bumped to 0.2 after
  first edit.
- **Gate:** no code-writing phase begins until the operator has
  signed off on the criteria. Signature mechanism: a commit
  containing `Acked-by: <operator> in escalation-criteria.md v0.2`
  in the message.

### Phase 1 — `refineforge-escalation` crate

- **Goal:** A pure-functional engine that, given a `ProposedAction`
  struct, returns `Decision::{Escalate(Category, Reason, Evidence), Proceed}`.
- **Scope:**
  - New workspace crate `crates/refineforge-escalation/`.
  - `Action` enum with one variant per kind of action the
    autonomous driver can take: `AddLeanModule`, `EditTheorem`,
    `MapRustToLean`, `WriteAxiom`, `WriteRefinementClaim`,
    `BumpStatus`, `WeakenTheorem`, `AssertExternalFact`,
    `BumpToolchainPin`, `AddDependency`, etc. (~15-20 variants).
  - `Category` enum mirroring the 8 categories in the criteria doc.
  - `Engine` struct with `decide(action: Action, project_ctx: &ProjectContext) -> Decision`.
  - `ProjectContext` reads claim YAMLs, Cargo.lock,
    lake-manifest.json so the engine knows what's "first time"
    vs not, what's in trust-base vs not.
  - **Pure functional.** No I/O inside `decide`. The caller
    populates `ProjectContext` once; the engine queries it.
- **Tests:**
  - One unit test per (category, positive-example) pair from
    the criteria doc → expect Escalate.
  - One unit test per (category, negative-example) pair → expect
    Proceed.
  - Multi-category test: an action that trips Categories 1 + 8
    simultaneously → expect Escalate with both listed.
  - Criteria-version-mismatch test: engine refuses to operate
    against a `ProjectContext` whose recorded criteria_version
    differs from the engine's compiled-in version.
  - Target: **≥ 60 unit tests** (8 categories × ~4 positive +
    ~3 negative examples + multi-category + edge cases).
- **Time:** 2 days.
- **Acceptance:**
  - All 8 positive examples from criteria-doc §3 produce
    Escalate with the right category.
  - All "do NOT escalate" examples produce Proceed.
  - `cargo nextest run -p refineforge-escalation` passes.
  - No `unsafe`, no `tokio`, no network — pure library.
- **Risks + mitigations:**
  - *Risk:* category boundaries are ambiguous in real code. *Mitigation:* every ambiguous case escalates (per doctrine §2); add the case as a "do escalate when unsure" example in criteria v0.2.
  - *Risk:* `ProjectContext` becomes a kitchen-sink. *Mitigation:* it ONLY holds data the engine actually queries; populate-once-query-many design.

### Phase 2 — Decision-packet generator + git checkpoint

- **Goal:** Generate a markdown packet that gives the human
  everything needed to decide in under 2 minutes; commit it to
  `escalations/<CLAIM-ID>/`; wait for a signature-commit; parse
  the decision.
- **Scope:**
  - `crates/refineforge-escalation/src/packet.rs` (same crate;
    keep the contract together).
  - `Packet` struct with all required fields per category (the
    "Decision packet contents" lists in criteria-doc §3).
  - Markdown renderer with one template per category — each
    template's structure is locked; only field values vary.
  - YAML front-matter recording: `criteria_version`,
    `claim_id`, `category`, `generated_at`,
    `generated_by_strategy`.
  - Evidence-collector trait — each category has a corresponding
    collector that reads the project context and produces a
    structured Evidence block (e.g. `IdealisationEvidence{
    rust_type, lean_type, lost_properties: Vec<…>,
    affected_theorems: Vec<TheoremId> }`).
  - `git`-aware checkpoint:
    - `commit_packet(packet) -> CommitSha` writes the packet,
      stages it, commits with a structured message
      `escalation: <category> for <claim_id>`.
    - `await_decision(packet_path) -> Decision` polls for new
      commits touching `packet_path` and parses the
      `## Human decision` section. Recognises `APPROVED`,
      `REJECTED`, `EDIT_AND_RESUBMIT`.
    - Polling interval: 5 seconds; timeout configurable, default 7 days.
- **Tests:**
  - Packet renderer: each category renders all required fields,
    no fields missing, markdown is syntactically valid.
  - Front-matter is parseable as YAML, round-trips through serde.
  - Mock-git: a fake git layer for tests so we don't need a real
    repo to exercise commit+await.
  - End-to-end (POSIX): real git tempdir, real commit, second
    process writes APPROVED, first process detects it.
  - Target: ~25 tests.
- **Time:** 2 days.
- **Acceptance:**
  - Every category has a packet template; every template fills
    every field listed in criteria-doc §3.
  - A human reviewer (operator) can read any packet and decide
    in ≤ 2 minutes (subjective; gate is operator's sign-off).
  - Decision detection works across git push/pull boundaries —
    the operator can decide on a different machine.
- **Risks + mitigations:**
  - *Risk:* polling git is slow / racey on large repos.
    *Mitigation:* short-circuit via filesystem mtime watch first;
    git log only as confirmation.
  - *Risk:* packet templates drift from criteria doc. *Mitigation:*
    a build-time check: parse criteria-doc §3 → assert each
    category has a packet template + every required field is in
    the template's struct.

### Phase 3 — `refineforge-autonomous` driver

- **Goal:** End-to-end `refine autonomous <CLAIM-ID>` subcommand.
  Plans the work for a claim, executes steps, escalates per Phase 1
  engine, waits per Phase 2 packet-loop, produces final bundle.
- **Scope:**
  - New workspace crate `crates/refineforge-autonomous/`.
  - `Planner` that reads claim YAML, decides which sequence of
    actions to attempt: load → draft model (if missing) → run
    repair (if broken) → write refinement doc (if model+refined)
    → run scan → export bundle.
  - `Executor` that runs each step, wrapping the existing
    `refineforge-cli` (claim, runner, scan, bundle) and
    `refineforge-strategies` (anthropic repair) machinery.
  - For each proposed step, calls
    `refineforge_escalation::decide`. On `Escalate`, generates a
    packet, commits, awaits decision; on `Proceed`, executes.
  - Final summary report: every escalation (count, decision,
    reasoning), every non-escalating action (categorised), full
    diff of the claim's files from start to finish.
  - New CLI: `refine autonomous <CLAIM-ID>
    [--strategy anthropic|anthropic-mock|mock]
    [--max-cost-usd 10.00]
    [--escalation-timeout-days 7]
    [--operator <name-or-email>]
    [--dry-run]`
  - Cost gate: track cumulative API spend; fail closed at
    `--max-cost-usd`.
- **Tests:**
  - Driver-level dry-run: against EXAMPLE-002, no real strategy
    call, no real git commit; assert planner produces N steps
    in the right order.
  - Driver with `mock` strategy: end-to-end on EXAMPLE-002,
    verify zero API calls + bundle produced.
  - Driver with `anthropic-mock` strategy: same.
  - Driver-with-escalation simulation: a forced-Category-2
    scenario verifies the packet is written and the driver
    blocks until a stubbed "APPROVED" appears.
  - Target: ~30 tests + 1 honest end-to-end on EXAMPLE-002
    using real Anthropic (cost-capped at $1).
- **Time:** 4 days.
- **Acceptance:**
  - Full happy path on EXAMPLE-002 produces a bundle with no
    escalations (file is already clean) in ≤ 30 s.
  - Forced-broken EXAMPLE-002 (with the wrong-tactic mutation)
    triggers AT MOST one Category-6 (theorem weakening) packet
    OR the repair loop closes the proof without escalation —
    the engine's decision is recorded either way.
  - All four sections (Lean / repair / scan / bundle) wire up;
    the trainer + bitexact crates are *out of scope* for the
    first driver release (added in a follow-up phase 3.5 once
    the Lean side is stable).
- **Risks + mitigations:**
  - *Risk:* the planner is brittle to claim YAMLs that don't
    match a canonical shape. *Mitigation:* validate-then-plan;
    surface schema errors as Phase 0 prerequisites, not
    autonomous failures.
  - *Risk:* the LLM proposes an action the engine's `Action`
    enum can't represent. *Mitigation:* every strategy returns
    a structured `Patch` already (`refineforge-repair-api`);
    extend Action enum case-by-case as needed.
  - *Risk:* runaway API cost. *Mitigation:* `--max-cost-usd` is
    enforced at every API-call boundary; fail closed if exceeded.

### Phase 3.5 — Section 2 (training) + Section 4 (bitexact) integration

- **Goal:** Extend the planner to drive `refine-train` and
  `refine-bitexact` autonomously.
- **Scope:** Add `Action::RunTrainingExperiment`,
  `Action::RunBitExactGate` and the corresponding planner edges.
- **Time:** 1 day (after Phase 3 is stable).
- **Acceptance:** A claim that has both an `rust_source` block
  AND a `training:` block AND a `kernels:` block (none today —
  this is forward-looking) can be driven end-to-end with
  escalations from each section as appropriate.
- **Note:** Phase 3.5 is parallelisable with Phase 4 by a second
  engineer.

### Phase 4 — EXAMPLE-002 dogfood + criteria v0.2

- **Goal:** Run the driver on EXAMPLE-002 with deliberate bait
  (the Counter `Nat`/`u64` idealisation), observe the
  escalation, refine the criteria doc based on what the human
  actually finds useful vs. noise.
- **Scope:**
  - Run `refine autonomous EXAMPLE-002 --strategy anthropic
    --operator galo@serragi.com` against a fresh tempdir copy.
  - Capture the produced packet(s); review for: clarity,
    completeness, decision-time-to-read.
  - Edit `docs/escalation-criteria.md` to v0.2 based on findings
    (likely refinements: clearer examples, possible category
    split / merge, time-based expiry decision from §4 open
    questions).
  - Re-run with v0.2 criteria; check that behaviour matches
    intent.
- **Time:** 1 day.
- **Acceptance:**
  - Operator can read the packet, decide, and resume the driver
    in ≤ 2 minutes per packet.
  - The final bundle is sign-able (passes `refine bundle verify`
    and the operator signs the tag commit).
  - Criteria-doc v0.2 lands with at least one named change from
    v0.1; if no change is needed, the v0.1 → v0.2 bump is the
    "no change after first review" certification.

### Phase 5 — Docs + integration + release ritual

- **Goal:** Polish, integrate, release.
- **Scope:**
  - README documentation map + framework build plan + subcommand
    table updated.
  - STRUCTURE.md updated.
  - CHANGELOG entry for `refine autonomous`.
  - SECURITY.md updated: autonomous driver is a new attack
    surface (decision packets ARE code-equivalent in their
    effect on the bundle); document trust assumptions.
  - `release/release.sh` rev-bump path validated.
- **Time:** ½ day.
- **Acceptance:** Everything reads cleanly to a cold reviewer.
  `release/release.sh 0.2.0 --dry-run` passes.

## 4. Total honest estimate

| Phase | Days | Cumulative |
|---|---:|---:|
| 0 — criteria doc | 0.5 | 0.5 |
| 1 — escalation engine | 2.0 | 2.5 |
| 2 — packet + git checkpoint | 2.0 | 4.5 |
| 3 — autonomous driver | 4.0 | 8.5 |
| 3.5 — trainer + bitexact integration | 1.0 | 9.5 |
| 4 — EXAMPLE-002 dogfood + criteria v0.2 | 1.0 | 10.5 |
| 5 — docs + release | 0.5 | 11.0 |
| **Total** | **11 working days** | |

= **~2 calendar weeks** with one focused engineer working 80 %
on this project (the other 20 % absorbs PR review, dependency
bumps, on-call interruptions).

## 5. Resource requirements

### People
- **1 engineer**: senior Rust + LLM-integration experience. Must
  know `syn` / `quote` patterns (Phase 1 — Action enum), `tokio`
  basics (Phase 2 — git polling), and prompt-engineering
  fundamentals (Phase 3 — strategy wrapping).
- **1 reviewer**: the operator (you). Time budget: ~30 min
  per phase for design review; ~1 hr for Phase 4 dogfood.

### Compute
- **Local development:** any developer laptop.
- **Anthropic API:** Phase 3 + 4 testing burns API credit.
  Honest estimate: **$50-150** over the 2 weeks based on
  ~500 dev-time API calls at the measured $0.07/call rate.
  Cap via `--max-cost-usd` flag.
- **GPU:** not required for Phases 1-4. Section 4 (bitexact)
  integration in Phase 3.5 only smoke-tests with the stub
  scripts — same as today's CI.

### External services
- **GitHub remote** (or any git remote): **NOT required**.
  The driver works against the local-only repo refineforge
  already is. A remote would enable cross-machine decision
  packets (the operator could decide on a different laptop)
  but the v0.2 contract permits same-machine-only.
- **Sigstore / cosign:** unchanged from today. The autonomous
  driver inherits the bundle-signing pipeline; no new keys.

## 6. Risks (project-level)

| Risk | Severity | Mitigation |
|---|---|---|
| Operator finds packets too verbose; starts skim-approving | **HIGH** — destroys the trust boundary | Per-category template length cap; pilot Phase 4 with stopwatch; iterate template format in v0.2 |
| Engine misclassifies an action (false Proceed); a Category-2 escalation gets silently skipped | **HIGH** — wrong claim ships | Multi-category overlap detection; conservative defaults; build-time cross-check criteria-doc ↔ engine code |
| Engine over-escalates (false Escalate); operator overwhelmed | **MEDIUM** — drives operator to disable | Phase 4 dogfood specifically measures escalation/hour rate; if > 10/claim, criteria v0.2 narrows examples |
| LLM strategy hallucinates an action the engine can't classify | **MEDIUM** — driver crashes or proceeds with wrong category | Treat unknown action shapes as Category-1 (Scope) by default; never silently auto-proceed |
| API cost overrun | **LOW** | `--max-cost-usd` enforced; default $10 cap |
| The first real autonomous run produces a packet so unclear the operator can't decide | **MEDIUM** | Phase 4 catches this; criteria-doc v0.2 fixes the template |
| Operator approves a packet, then changes their mind after merge | **LOW** but recurring | Approval is a commit; revert is a commit; record both. The bundle's `report.json` already chains them. |

## 7. Definition of done (whole project)

All of:

1. ✅ `docs/escalation-criteria.md` v0.2 signed off by operator.
2. ✅ `crates/refineforge-escalation` ships with ≥ 60 unit tests
   covering every (category, positive + negative) pair.
3. ✅ `crates/refineforge-autonomous` ships with `refine
   autonomous` subcommand wired into the main `refine` binary.
4. ✅ Forced dogfood on EXAMPLE-002 produces ≥ 1 packet of the
   expected category; operator signs; bundle ships.
5. ✅ Forced dogfood on EXAMPLE-001 (Lean-only, no idealisation
   bait) produces ZERO escalations; bundle ships.
6. ✅ `cargo nextest run --workspace` passes (target: 250+ tests).
7. ✅ README / STRUCTURE / CHANGELOG / SECURITY all updated.
8. ✅ One commit per phase (5-7 commits total) for clean revert.
9. ✅ `release/release.sh 0.2.0 --dry-run` passes.

## 8. Out of scope (explicitly)

These DO NOT land in v0.2 even if they'd be useful — keep the
build focused:

- **Multi-claim parallel autonomous runs.** v0.2 runs one claim
  at a time. Multi-claim is a v0.3 feature.
- **Cross-machine decision packets.** v0.2 requires the operator
  to decide on the same machine as the driver. Cross-machine
  needs a git remote and conflict resolution — v0.3+.
- **Approve-via-Slack / email.** v0.2 is git-only. Slack
  integration is a v0.3+ add-on.
- **LLM-tuned escalation criteria.** The criteria are
  human-edited only. An AI suggesting criteria edits would
  create a circular trust loop — v0.4 or never.
- **Autonomous TRAINING runs.** Phase 3.5 wires the driver to
  `refine-train`, but only as much as a dry-run plan. Actually
  burning $50-500 of GPU time without a human is out of scope.
- **Real CUDA-kernel autonomous edits.** Same reason.
- **Auto-merge of approved packets** (e.g. via a GitHub Action).
  The operator's commit is the approval; auto-merging would
  bypass the local review step.

## 9. Open questions for the operator BEFORE Phase 1 starts

The 4 open questions from `escalation-criteria.md` §"Open
questions" must be resolved before code lands, because
each affects the engine's design:

1. Mathlib first-use: separate category or merged into Scope?
2. Bit-exact regression: separate category or implicit via Categories 2+6?
3. Time-based escalation expiry: default 7 days or indefinite?
4. Batch escalations: one packet per idealisation or one packet listing all idealisations?

Decisions go into criteria-doc v0.2 BEFORE Phase 1.

## 10. Failure-mode rehearsal (red team)

Before claiming the project succeeded, walk through each:

- **The "rubber-stamp" failure mode.** What does it look like
  when the operator stops reading packets? Detection: log every
  decision's median-time-to-decision; alert if it drops below
  30 seconds for 3 consecutive packets. Response: criteria-doc
  edit to make packets harder to skim.
- **The "silent drift" failure mode.** What does it look like
  when the engine evolves to allow what previously escalated?
  Detection: criteria-doc version recorded in every packet;
  CI job that diffs the doc on every push and surfaces
  weakenings to the operator. Response: revert.
- **The "uneditable contract" failure mode.** What does it look
  like when the criteria are impossible to change without
  breaking in-flight claims? Detection: Phase 4 specifically
  tests a criteria-doc edit mid-claim. Response: design v0.2
  with explicit "restart this claim under new criteria"
  semantics.
- **The "false-success" failure mode.** What does it look like
  when the autonomous driver produces a bundle that passes all
  checks but is semantically wrong? This is exactly the
  refinement-doc's job to prevent. Defence: Category 4
  (Refinement-doc claims about customer intent) ALWAYS escalates;
  the human's signature is on the claim's truth, not the AI's
  craftsmanship.

---

## After Phase 5: what to build next

The autonomous driver is a multiplier. With it shipping, the
remaining items on refineforge's roadmap become more interesting
because each can be driven autonomously up to the escalation
boundary:

- **Mathlib mutation pipeline** for the eval corpus (Section 2
  phase 1 item 3 in `docs/repair-evaluation.md` §9) — can be
  driven autonomously: scrape Mathlib (Category 1 — scope),
  apply mutations (no escalation), produce corpus
  (no escalation), commit (no escalation), train (escalate on
  cost gate + Category 8 trust-base).
- **Hardware-backed release-tag signing** (Section 3 phase 3).
- **Pure-Rust `--verify-signature`** (drops cosign dep).
- **Real Mathlib-using claim** to exercise the new bundle
  support end-to-end.

But none of those are in this plan's scope. Each is a separate
multi-day engagement after `refine autonomous` lands.
