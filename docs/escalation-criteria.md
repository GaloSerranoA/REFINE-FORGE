# Escalation criteria — the AI-to-human contract for `refine autonomous`

> **Status:** v0.2 — operator-signed. These criteria are the
> boundary between autonomous AI action and human judgment for
> the `refine autonomous` driver
> (`crates/refineforge-escalation/` ships in this revision; the
> wrapping driver in `crates/refineforge-autonomous/` is still
> pending — see [`autonomous-driver-plan.md`](autonomous-driver-plan.md)).
>
> Every escalation packet records the **version** of this doc the
> AI was operating under. Changes to this doc are themselves
> scope changes (Category 1) — review carefully.

## What this doc is for

The `refine autonomous` driver does Lean / ML / DevOps / CUDA
engineering work without per-step human approval, escalating to
a named human operator ONLY when its proposed next action falls
into one of the **nine categories** below. The operator approves
or rejects via a decision packet committed to `escalations/<CLAIM-ID>/`.

This is **human-on-the-loop**, not human-in-the-loop. The human
signs the bundle at the end and signs each escalation packet;
the AI does everything else.

## Doctrine

These criteria are non-negotiable defaults:

1. **Categorical, not numerical.** No "confidence threshold." Either
   the action matches a category and escalates, or it doesn't. The
   AI does not get to assess its own confidence and self-suppress
   an escalation.
2. **Conservative by default.** When unsure whether a category
   applies, escalate. The cost of an unnecessary escalation is one
   minute. The cost of a missed escalation is a wrong claim under
   the operator's signature.
3. **Categories are versioned.** This document carries a version
   number. Every escalation packet records the criteria version it
   was generated against. Editing the criteria for a claim already
   in flight requires restarting the claim's escalation cycle
   under the new version.
4. **Adding or removing a category is itself a Category-1 (Scope)
   escalation.** The criteria contract evolves only with explicit
   human decision-packet approval.
5. **Each escalation produces a decision packet** with: question,
   options, AI's recommendation, evidence the AI consulted, what
   the AI could not determine, dissenting consideration, criteria
   version. Cheap escalations destroy the system; expensive
   escalations make it work.
6. **Final signature is always human.** Even if zero escalations
   fire during a claim's life, the bundle's tag signature
   (`release/release.sh` + cosign tag-commit) is signed by the
   human, not the AI.

## The 9 escalation categories

Each category has: definition · examples that DO escalate · examples
that DO NOT escalate · what the decision packet must contain.

---

### Category 1 — Scope change

**Definition.** The AI proposes to add, remove, or restructure a
Lean / Rust / config entity that is not listed in the claim's
stated scope. Includes first-time-in-project structural choices
(first use of an inductive predicate for a state machine, first
use of `BitVec`, first Mathlib tactic, first sweep config beyond
the existing dimensions).

**Escalate when:**
- Claim YAML `scope: model-only` and AI wants to add a Rust
  `rust_source` block.
- Claim's `rust_source.types` lists `[Counter, incr]` and the AI
  wants to add `Capability`.
- Claim's `lean.theorems` lists `[t1, t2]` and the AI wants to
  add `t3`.
- AI wants to introduce a new Lean module that isn't imported
  from the library root.
- First time the AI uses a Mathlib import in this project
  (`Mathlib.X` not previously in `lake-manifest.json`).
- AI wants to add a new workspace crate, a new top-level
  directory, or a new template under `templates/`.
- AI wants to add a new ANTHROPIC_MODEL value, a new training
  backend, a new bit-exact-gate target — anything that expands
  the surface the human has previously approved.

**Do NOT escalate when:**
- Adding a `--help` description to an existing CLI flag.
- Renaming a local variable inside a proof body.
- Reformatting (no semantic change).
- Adding test cases for already-listed theorems (test additions
  reinforce existing scope, don't expand it).

**Decision packet contents:**
- The exact additions / deletions (a unified diff).
- The claim YAML field the change affects.
- The smallest in-scope alternative the AI considered and why
  it was rejected.
- Any sibling claims that would also need updating if the change
  lands.

---

### Category 2 — Idealisation

**Definition.** The AI maps a Rust type to a Lean type that
**loses information**. The escalation must name what is lost and
under what theorems the loss would matter.

**Escalate when:**
- `u8/u16/u32/u64/usize` → `Nat` (loses overflow / bounded
  arithmetic). The Counter / EXAMPLE-002 idealisation.
- `i8/i16/i32/i64/isize` → `Int` (loses bit-width).
- `[u8; N]` → `Nat` or `String` (loses fixed-width property).
- `RwLock<T>` / `Mutex<T>` / `Arc<T>` → `T` (loses
  concurrent-access story; serialisation is assumed externally).
- `Result<T, E>` → `T` (loses failure path).
- `Option<T>` → `T` (loses absence).
- Floating-point (`f32`/`f64`) → `Rat` or `Real` (loses
  rounding; introduces an idealisation that no real CPU
  satisfies).
- Anything using the `derive(LeanModel)` proc-macro's idealisation
  mapping (which is exactly this category enforced mechanically).

**Do NOT escalate when:**
- `String` → `String` (UTF-8 byte sequence ↔ UTF-8 byte sequence;
  no loss for code that doesn't depend on capacity/allocator).
- `bool` → `Bool` (exact correspondence).
- Same-shape struct → Lean `structure` with same fields
  (no information lost).

**Decision packet contents:**
- The Rust → Lean mapping.
- The specific information being lost (overflow / bit-width /
  absence / failure / concurrency / rounding).
- Which of the claim's stated theorems WOULD be invalidated by
  the loss in a real adversarial input.
- An alternative mapping that preserves the information, with
  cost estimate (e.g., "switch to `BitVec 256` — multiplies
  proof effort by ~5×").
- The strongest argument for accepting the idealisation in this
  claim's stated scope.

---

### Category 3 — Custom axiom

**Definition.** Any `axiom` declaration in a Lean source file
that is NOT one of Lean core's canonical axioms (`propext`,
`Classical.choice`, `Quot.sound`). The `refineforge` policy gate
already rejects this by default; an escalation either (a) gets
the human to override the policy in the claim YAML or (b)
rewrites to avoid the axiom.

**Escalate when:**
- AI writes `axiom hash_is_injective : ∀ a b, hash a = hash b → a = b`.
- AI writes `axiom rust_libc_is_correct : ...`.
- AI writes ANY axiom statement, including ones the AI considers
  "obviously true."

**Do NOT escalate when:**
- AI uses existing axioms from Lean core (already permitted).
- AI uses lemmas marked `axiom` in Mathlib (those are imported,
  not declared in our source — but adding the Mathlib import
  itself trips Category 1).

**Decision packet contents:**
- The exact axiom statement.
- What would be true / false in the real world if the axiom is
  wrong.
- Whether a sibling claim (e.g., `<PROJECT>-CRYPTO-*`) already
  cites the same axiom, and if so, the citation.
- Whether the axiom can be replaced with a proof using stronger
  assumptions; cost of doing so.
- The exact `policy.no_axioms_beyond_lean_core: false` change
  to the claim YAML that the human is approving (one-line diff).

---

### Category 4 — Refinement-doc claim about customer intent

**Definition.** The AI is about to write text in a refinement doc
(`docs/refinement/<CLAIM-ID>.md`) that claims what HELYX
**customers**, **end users**, **regulators**, or any **external
party** understands a concept to mean. The AI has not met any of
these people.

**Escalate when:**
- "Customers expect `revoke` to take effect within 1 second."
- "Operators interpret this claim as covering both audit AND
  replay scenarios."
- "This matches the OWASP definition of XSS prevention."
- "Per the GDPR right-to-erasure, this counts as erasure."
- "In the threat model HELYX customers have, X is not in scope."
- ANY sentence in a refinement doc starting with
  "Customers/users/regulators/operators expect …"

**Do NOT escalate when:**
- Describing what the Lean theorem proves (mathematical statement
  about the model — verifiable from the source).
- Describing what the Rust code does (verifiable from the source).
- Citing a fact from a doc the human has already approved
  ("Per `docs/methodology.md` §4 …").

**Decision packet contents:**
- The exact sentence the AI wants to write.
- The strongest source the AI can cite (RFC, contract, prior
  human approval, sibling refinement doc); explicitly "I could
  not find a source" is a valid answer.
- Two alternative framings: one stronger ("X is true"),
  one weaker ("X is consistent with our reading of …").
- The deployment context the claim is meant to cover (if known)
  and whether the assertion is necessary for that context.

---

### Category 5 — Status upgrade

**Definition.** The AI wants to change a claim YAML's `status:`
field. Especially: `proven` (model-only) → `proven` (model+refined),
which is the moment a claim becomes marketable as a verified
artifact about the running system.

**Escalate when:**
- AI wants `proven` (model-only) → `proven` (model + refined)
  on any claim.
- AI wants `broken` → `drafted` (rescuing a previously-failed
  claim).
- AI wants `drafted` → `proven` (model-only) — i.e., the first
  time a claim's proofs all pass.
- AI wants to flip `review.human_operator` from `null` to any
  value.

**Do NOT escalate when:**
- AI wants `unformalized` → `drafted` (just an intent change,
  no claim of correctness yet).
- AI is updating non-status fields (`description`, `authors`).

**Decision packet contents:**
- The current and proposed `status:` values.
- Every machine-checkable item from the claim's reviewer
  checklist with its current pass/fail state.
- Every `[needs human]` reviewer-checklist item with its current
  state (pending / addressed / N/A).
- The git diff of the claim YAML the human is signing.
- The exact `review.human_operator` value to write
  (the human's identity).

---

### Category 6 — Theorem deletion or weakening

**Definition.** The AI couldn't prove an originally-stated theorem
and wants to delete it or weaken its statement.

**Escalate when:**
- Original `theorem t : ∀ x, P x`. AI proposes
  `theorem t : ∀ x : Subset, P x`.
- Original `theorem t : ∀ x, P x ∧ Q x`. AI proposes
  `theorem t : ∀ x, P x` (drops the conjunct).
- Original `Hash a = Hash b → a = b` (collision-free). AI
  wants to delete or replace with `Hash a = Hash b → a ≈ b`.
- Original `(incr c).value > c.value`. AI wants
  `(incr c).value ≥ c.value` (the EXAMPLE-002 strict-vs-monotone
  trade-off).

**Do NOT escalate when:**
- AI restructures the proof body (same statement, different
  tactics).
- AI factors a proof into helper lemmas (same conclusion).
- AI renames a theorem (refactor, no semantic change).
- AI inserts `sorry` — this is REJECTED outright by the policy
  gate; no escalation possible.

**Decision packet contents:**
- The original theorem statement.
- The proposed weakened statement.
- The specific proof obligation the AI could not discharge
  (the unsolved goal Lean reports).
- What the weaker version DOES still prove vs. DOES NOT.
- The downstream callers (other theorems, refinement-doc claims)
  that depended on the original strength.
- Whether HELYX-level claims that cite this theorem need
  updating, and which.

---

### Category 7 — External-fact assertion

**Definition.** The AI is about to make a claim about the real
world that cannot be verified from the repository's own contents
(source code, tests, lake-manifest, claim YAMLs).

**Escalate when:**
- "The `sha2` crate implements SHA-256 per FIPS 180-4." (AI
  cannot verify the crate's behaviour from `Cargo.lock` alone.)
- "This matches the algorithm in RFC 5246 §6.2.3." (AI cannot
  read the RFC.)
- "HELYX's production system uses `Mutex<T>` not `RwLock<T>`."
  (AI cannot read deployed configuration.)
- "The CUDA driver version 550.54.15 is bit-exact for this
  kernel." (AI cannot run the kernel on real hardware.)
- "The Lean kernel has no known soundness bugs as of v4.29.1."
  (AI cannot verify upstream issue trackers.)
- ANY citation of an external standard, RFC, paper, or
  vendor-claim that the AI hasn't been given the text of.

**Do NOT escalate when:**
- Citing facts from files in the repo
  ("Per `lean/lean-toolchain` we pin v4.29.1").
- Citing facts from the claim YAML
  ("The claim's `lean.theorems` list includes …").
- Citing facts from prior decision packets the human signed.

**Decision packet contents:**
- The exact assertion.
- What the AI's source for it was (training data / search /
  inferred from filename).
- What the AI checked vs. what it couldn't check
  ("I checked Cargo.lock pinned `sha2 = "0.10.9"` from
  RustCrypto. I did NOT verify that 0.10.9 implements SHA-256
  correctly.").
- A weakened reformulation the AI would write if the human
  doesn't want to accept the strong form
  ("This claim assumes `sha2 = "0.10.9"` correctly implements
  SHA-256; that assumption is itself the subject of
  `<PROJECT>-CRYPTO-*`.")

---

### Category 8 — Trust-base extension

**Definition.** The AI wants to add, change, or upgrade something
in the project's **trusted code base** (TCB) — the things every
claim conditionally depends on for its correctness. Even when the
change looks routine.

**Escalate when:**
- Bumping `lean-toolchain` from v4.29.1 to any other version.
- Adding a Mathlib (or other Lake package) dependency.
- Bumping any pin in `Cargo.lock` for crates in the bundle's
  trust chain (`sha2`, `serde`, `regex`, `lsp-types`, `reqwest`,
  `sigstore-rs` if added, etc.).
- Switching a crate (e.g., `serde_yaml` → `yaml-rust`).
- Changing the cosign version pinned in CI.
- Changing the SHA of any `*-installer-action@*` GitHub Action.
- Adding a new system tool to the verifier Dockerfile.
- Pointing `refine repair --strategy anthropic` at a different
  ANTHROPIC_MODEL than previously approved.

**Do NOT escalate when:**
- Updating a dev-dependency that doesn't appear in any bundle
  (e.g., `tempfile`, `tiny_http`).
- Updating a `proc-macro` like `syn` that only runs at build
  time and doesn't end up in the binary — but DO escalate if a
  refinement-doc cites it.
- Changing test fixtures.

**Decision packet contents:**
- The current pin and proposed pin (with hash if available).
- The transitive change-set the bump pulls in
  (e.g., `cargo tree --invert` output for the affected crate).
- Reason for the bump (security advisory, feature need,
  upstream API change).
- Sibling claims that need re-verification after the bump.
- Whether the bump invalidates any signed bundles already
  produced; if so, which.

---

### Category 9 — Bit-exact regression

> **Added in v0.2.** Resolution of open question §2 from v0.1 —
> bit-exact regressions are split out from Categories 2 + 6 because
> the harm shape is qualitatively different (a previously passing
> kernel starts producing divergent SHA-256 across runs without any
> source-level theorem changing), and the decision the operator
> needs to make is different too (re-baseline vs revert vs accept
> hardware-class-specific divergence).

**Definition.** The AI proposes a change that affects, or could
affect, the bit-exactness contract a `refine-bitexact` gate has
previously certified. The contract is: "this kernel produces
byte-identical output across N independent runs on the
operator-named hardware class."

**Escalate when:**
- AI edits any file under `kernels/src/`.
- AI changes `kernels/configs/<kernel_id>.yaml` `run_count`,
  `output:` shape, or the `command` invoked.
- AI bumps the compiler / runtime pin used to build a kernel
  (nvcc version, CUDA toolkit, cuDNN, ROCm, Metal driver).
- AI changes `kernels/scripts/<kernel_id>.sh` or `.ps1` in any
  way that affects the produced bytes (env, args, ordering of
  ops).
- AI proposes a build-flag change for a kernel
  (`-arch=sm_<X>`, `-O<N>`, fast-math toggles).
- AI proposes adding a `kernels/<NEW_KERNEL>/` directory
  (also trips Category 1 — scope).
- AI proposes lowering `run_count` below the baseline value
  the gate previously passed at (would mask divergence).

**Do NOT escalate when:**
- AI updates the kernel's README, comments inside its source
  (with no executable change), or docs adjacent to it.
- AI changes a kernel-adjacent test fixture that is NOT
  hashed by the gate.
- AI raises `run_count` above the baseline (strictly more
  evidence; cannot turn a Pass into a Fail except by
  catching real non-determinism, which is the desired outcome).

**Decision packet contents:**
- The kernel id and its current passing baseline (run_count,
  baseline SHA-256 set, hardware class the operator named).
- The exact diff (source / build-flag / pin / config) the AI
  wants to apply.
- Predicted impact: "no bit change expected" / "bit change
  expected — re-baseline needed" / "unknown — needs run on
  real hardware before/after."
- Sibling claims whose refinement docs cite the kernel's
  bit-exactness, if any.
- Whether `docs/bit-exact-reproducibility.md` §X needs
  updating (e.g., a newly discovered non-determinism source
  added to the table).
- The strongest argument for the change (security advisory,
  performance need, upstream fix).

---

## Meta-rules

### Multiple categories simultaneously

If a proposed action trips more than one category, the escalation
packet **lists all of them** and uses the most-restrictive's
required decision-packet contents. Example: adding a Mathlib
dependency to a `model-only` claim trips Category 1 (scope) AND
Category 8 (trust-base); the packet documents both.

### When the AI is uncertain whether a category applies

Escalate. The decision packet should explicitly say "I am
unsure whether this trips Category N." The human's response can
adjudicate AND, if recurring, motivate a doc edit to clarify the
category.

### What counts as "the AI"

In `refine autonomous`'s context, "the AI" is the planner +
strategies + repair loop + drafting helpers that operate without
human input. A `mock` strategy is still "the AI" — its proposed
actions go through the engine.

### Escalation packet location

```
escalations/<CLAIM-ID>/<timestamp>-<category>-<topic>.md
```

Example:
```
escalations/HELYX-AUDIT-001/2026-05-18T203045Z-idealisation-u64-to-nat.md
```

The packet is a markdown file committed to git. The AI watches for
the human's signature line (e.g., `## Human decision\n\nAPPROVED:
<reason>` or `REJECTED: <reason>`). The autonomous driver continues
when the signature commit lands.

### Criteria version recording

Every packet has a YAML front-matter block:

```yaml
---
criteria_version: 0.1
claim_id: HELYX-AUDIT-001
category: idealisation
generated_at: 2026-05-18T20:30:45Z
generated_by_strategy: anthropic
---
```

If `docs/escalation-criteria.md` is edited mid-claim, the
autonomous driver halts in-flight claims, surfaces the diff to
the human, and restarts the affected claims under the new
version.

### Removing or weakening a category

Removing a category from this doc, or weakening its scope, is
itself a Category-1 (scope) escalation against the CONTRACT
between AI and human. Document it in `CHANGELOG.md` with the
removal date and the rationale. Once removed, the category cannot
be silently re-added — a re-addition is a new Category-1
escalation.

### Escalation expiry (v0.2)

> Resolution of open question §3 from v0.1.

The autonomous driver waits on each packet for **7 calendar
days** by default before treating it as auto-rejected and
shutting the claim down (the driver writes a `STATUS:
EXPIRED-AUTO-REJECTED` line into the packet and stops). Expiry
is **configurable per-category**: each category may override the
default via `--escalation-timeout-days.<category>=<N>` on the
driver invocation, or via a `[escalation_timeout_days]` table in
the operator's local config. The minimum permitted value is
**1 day** (an operator who needs faster turnaround should be
present, not racing a timer); the maximum is **30 days** (beyond
that, the work has stalled and re-planning is cheaper than
waiting).

Rationale for the default: 7 days absorbs a typical
holiday/conference week without forcing a rushed decision, while
preventing packets from accumulating forever. The per-category
override exists because (a) Category 3 (custom axiom) and
Category 5 (status upgrade) often warrant longer review than
Category 1 (scope), and (b) Category 9 (bit-exact regression) is
often time-sensitive because the kernel change is blocking a
release.

Expiry is recorded in the packet's YAML front-matter as
`expires_at: <ISO 8601 timestamp>` so the operator sees the
deadline at the top of every packet they open.

### Batch escalations (v0.2)

> Resolution of open question §4 from v0.1.

If an AI step proposes a single coherent action that trips
multiple categories simultaneously (the "Multiple categories
simultaneously" rule above), that is **one packet** listing all
tripped categories.

If an AI step proposes N independent actions (e.g., 5 separate
idealisations spread across 5 different Rust→Lean mappings, each
escalatable on its own), that is **N packets** — one per item.
Rationale: per-item context is more important than brevity. The
operator deciding on idealisation #3 should not be visually
crowded by idealisations #1, #2, #4, #5 in the same view. A
batched packet biases toward the operator approving or rejecting
all 5 together when they would have approved 3 and rejected 2.

If the operator finds themselves drowning in per-item packets for
a single claim, that is a signal to either (a) restructure the
claim into smaller scoped claims, or (b) re-examine the criteria
to see whether one category is firing too eagerly — both are
operator decisions, not AI ones.

## Version history

| Version | Date | Change | Approved by |
|---|---|---|---|
| 0.1 | 2026-05-18 | Initial draft. 8 categories from the supervised-autonomy design conversation. Pending operator review before any code enforces them. | (pending) |
| 0.2 | 2026-05-18 | Operator-signed. Added Category 9 (bit-exact regression — resolution of open question §2). Merged "first-time Mathlib use" into Scope (resolution of §1). Added Meta-rule "Escalation expiry" with 7-day default + per-category override (resolution of §3). Added Meta-rule "Batch escalations" — N independent items = N packets, single coherent multi-category action = 1 packet (resolution of §4). | galo@serragi.com |

## Open questions

Resolved in v0.2 (see Version history). New open questions
discovered during future operator reviews land here.

(None at v0.2.)
