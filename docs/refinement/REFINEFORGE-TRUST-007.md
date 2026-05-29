# REFINEFORGE-TRUST-007 — Refinement argument: no-sorry policy gate

> **Status: model-linked (NOT human-reviewed).** Dogfood claim about the
> framework's *foundational* gate — the no-sorry policy gate that gives every
> `status: proven` claim its meaning. `review.human_operator` is `null` →
> `model-linked`.

## 1. What the Lean model says

In `lean/Refineforge/PolicyGate.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `Policy` | structure | the three flags `noSorry`, `noAdmit`, `noAxioms` |
| `Counts` | structure | `sorryCount`, `admitCount`, `axiomCount` (post-strip) |
| `ok` | function | accept iff no enabled flag has a positive count |
| `present_sorry_rejected` | theorem | **T1:** enabled `no_sorry` + a `sorry` ⇒ reject |
| `present_admit_rejected` | theorem | **T2:** enabled `no_admit` + an `admit` ⇒ reject |
| `present_axiom_rejected` | theorem | **T3:** enabled `no_axioms` + an `axiom` ⇒ reject |
| `clean_source_accepted` | theorem | **T4:** zero forbidden tokens ⇒ accept (any policy) |
| `default_policy_rejects_one_sorry` | theorem | **T5:** default policy + one `sorry` ⇒ reject |

T1–T3 are the safety direction (forbidden tokens can't slip through an enabled
policy); T4 shows the gate is not vacuously rejecting; T5 is the concrete
headline guarantee.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `struct GateResult` | struct | `crates/refineforge-cli/src/sorry_gate.rs` | (`ok` + `Counts`) |
| `fn check` | function | `…/sorry_gate.rs` | `ok` |
| `fn strip_comments` | function | `…/sorry_gate.rs` | (idealised — see §3/§5) |

`refine scan check REFINEFORGE-TRUST-007` confirms all three symbols exist.

## 3. Mapping

Rust `check` strips comments, counts `\bsorry\b`, `\badmit\b`, and `^\s*axiom\b`,
then sets `ok = true` and flips it to `false` whenever an *enabled* policy flag
has a positive count. `ok p c` models exactly that boolean:
`!(noSorry && sorry>0) && !(noAdmit && admit>0) && !(noAxioms && axiom>0)`.
**Idealisation:** the model takes the post-strip `Counts` as given. The regex
matching and `strip_comments` that *produce* those counts are **not** modelled —
so the Lean proves "given correct counts, the verdict is correct", not that the
lexer counts correctly.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; `rustc`/LLVM; the Rust standard library; and
**the `regex` crate** (the actual token matching) plus `strip_comments`. We make
no claim that any is itself verified — see §5.

## 5. What this claim does NOT cover

- **The lexer.** Whether the word-boundary regexes and `strip_comments` produce
  the *correct* counts is the load-bearing thing this claim does NOT prove. E.g.
  the Rust deliberately does **not** strip string literals (a `"sorry"` inside a
  string still counts — flagged for human review); nested block comments and the
  `^\s*axiom` anchor are regex behaviour, not modelled here.
- Core axioms: Lean's `propext`/`Classical.choice`/`Quot.sound` are not
  *declared* in claim source, so they don't appear as `axiom` lines; the gate
  counts user `axiom` declarations. The model treats `axiomCount` as those.
- That `check` is actually run before `lake build` (the caller's responsibility).

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-007` → `Verified`.
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-007` → `Verified`.
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-007`.
- [ ] **[needs human]** The Rust `ok` computation in `check` matches Lean `ok`
      (the three `!(flag && count>0)` conjuncts).
- [ ] **[needs human]** Taking the post-strip `Counts` as given (the regex +
      `strip_comments` lexer is out of scope, §5) is acceptable for the
      guarantee being claimed.

Once certified, populate `review.human_operator`; the claim moves to
`human-reviewed` / `model+refined`.
