# EXAMPLE-002 — Refinement argument

> **Status of this document:** TUTORIAL. Unlike a real refinement
> doc, this one is shipped pre-filled to demonstrate the pattern.
> It is the worked answer-key for what `docs/refinement-template.md`
> asks operators to write for their own claims. The Lean model + Rust
> impl are deliberately small so the bridge is auditable in five
> minutes.

## 1. What the Lean model says

In [`lean/Refineforge/Counter.lean`](../../lean/Refineforge/Counter.lean) we define:

| Lean entity                | Kind         | Meaning                                                  |
|----------------------------|--------------|----------------------------------------------------------|
| `Counter`                  | structure    | A natural-number counter: `value : Nat`                  |
| `incr`                     | function     | Adds 1: `{ value := c.value + 1 }`                       |
| `incr_monotone`            | theorem      | `(incr c).value ≥ c.value`                               |
| `incr_strictly_increases`  | theorem      | `(incr c).value > c.value`                               |

Both theorems are proven on `Nat` (Lean's unbounded naturals). The
unboundedness is load-bearing for `incr_strictly_increases` — see §3.

## 2. What the Rust must implement

| Rust entity      | Kind     | Path                                                              | Lean counterpart |
|------------------|----------|-------------------------------------------------------------------|------------------|
| `Counter`        | struct   | [`crates/example-counter/src/counter.rs:14`](../../crates/example-counter/src/counter.rs) | `Counter`        |
| `fn incr`        | function | [`crates/example-counter/src/counter.rs:33`](../../crates/example-counter/src/counter.rs) | `incr`           |
| `fn checked_incr`| function | [`crates/example-counter/src/counter.rs:39`](../../crates/example-counter/src/counter.rs) | (Rust-side only — opt-in strict semantics; see §3.3) |

`refine scan check EXAMPLE-002` reports `Verified, types=1/1 fns=1/1`.
(`checked_incr` is not named in the claim's `rust_source` block, so
the scan ignores it.)

## 3. Mapping

### 3.1 `Counter` ↔ `Counter`

**Implemented at:** [`crates/example-counter/src/counter.rs:14-24`](../../crates/example-counter/src/counter.rs).

- Rust `value: u64` ↔ Lean `value : Nat`.
- **Idealisation A (bit-width):** Lean `Nat` is unbounded; Rust
  `u64` saturates at `2^64 − 1`. For HELYX-AUDIT-001 we made a
  similar trade-off for `Hash` arguing it was safe because no
  theorem depended on arithmetic. For EXAMPLE-002 the situation is
  more interesting: one theorem (`incr_monotone`) IS preserved and
  one theorem (`incr_strictly_increases`) is NOT preserved at the
  boundary. §3.3 below treats this in detail.
- Field is private; the only public constructors are
  `Counter::new()` (starts at 0) and `Counter::from_value(u64)`
  (for testing and deserialisation). There is no public `set_value`
  — the type is mutated only through `incr` / `checked_incr`.
- `#[derive(PartialOrd, Ord)]` mirrors Lean's `Nat` ordering: two
  counters are compared by `value`. This is consistent with the
  Lean theorems' use of `≥` and `>`.

### 3.2 `incr` ↔ `incr` — monotonicity (`incr_monotone`)

**Implemented at:** [`crates/example-counter/src/counter.rs:33-35`](../../crates/example-counter/src/counter.rs).

- Lean: `incr c := { value := c.value + 1 }`.
- Rust: `Counter { value: c.value.saturating_add(1) }`.
- **Lean theorem `incr_monotone`** (`(incr c).value ≥ c.value`):
  Rust refinement is exact. `u64::saturating_add(1)` returns either
  `value + 1` (when `value < u64::MAX`) or `u64::MAX` (when
  `value == u64::MAX`). In both cases the result is `≥ value`. The
  test `incr_is_monotone_at_u64_max` exercises the boundary case.

### 3.3 `incr` ↔ `incr` — strict-increase (`incr_strictly_increases`)

**This is where the tutorial demonstrates a real idealisation gap.**

- Lean theorem `incr_strictly_increases` (`(incr c).value > c.value`)
  is proven in Lean by `simp [incr]` — straightforward because Lean
  `Nat` is unbounded.
- Rust `saturating_add` does NOT preserve strict increase at the
  boundary: when `c.value == u64::MAX`, `incr(&c).value() ==
  u64::MAX`, which is NOT greater than `c.value()`. The test
  `incr_does_not_strictly_increase_at_u64_max` documents this.
- **What to do about it.** Three options, in order of how
  conservative the refinement claim becomes:
  1. **Accept the gap (current choice).** Document the boundary;
     the deployment is responsible for ensuring counters are
     reset/rolled before reaching `u64::MAX`. At 1 billion
     increments/sec it would take ~584 years to reach the boundary,
     so for most counters this is acceptable. The refinement claim
     is then: "*for all `c` with `c.value < u64::MAX`, the Rust
     `incr` strictly increases.*" This is a weakening of the Lean
     theorem.
  2. **Use `checked_incr` (line 39) and require all callers to
     handle `None`.** Then the Rust API encodes the precondition
     `c.value < u64::MAX` in the type system. The Lean theorem
     transfers exactly.
  3. **Switch the Lean model from `Nat` to `BitVec 64`** and re-prove
     monotonicity with explicit boundary handling. Most expensive,
     but the model and the code then have identical arithmetic.

The current tutorial takes option 1 and documents the gap (this
section) plus provides `checked_incr` so option 2 is one line away.

### 3.4 `checked_incr` (Rust-side only)

`checked_incr` returns `Option<Counter>` — `Some` if increment was
safe, `None` at the boundary. It is not refined from any specific
Lean theorem; it exists so callers who need strict semantics can
opt in. A future EXAMPLE-003 could add a Lean theorem about
`Option`-returning incrementation and refine it directly.

## 4. Trusted code base

EXAMPLE-002 depends on:

1. **Lean's kernel** (~6000 LoC OCaml/C++).
2. **The Lean compiler v4.29.1** (pinned in `lean-toolchain`).
3. **`rustc` and LLVM**.
4. **The Rust standard library** — specifically `u64::saturating_add`
   and `u64::checked_add`, which are intrinsics with well-defined
   semantics.
5. **The OS and hardware** — beyond the scope.

There is no cryptographic primitive or external crate; the trusted
base is minimal.

## 5. What this claim does NOT cover

- **Concurrent increment.** The Rust API is immutable (`incr` takes
  `&Counter` and returns a new `Counter`). Concurrent reads are
  safe. If you wrap `Counter` in `AtomicU64` and use
  `fetch_add(1, Ordering::Relaxed)`, the monotonicity theorem
  still holds individually per increment — but the *ordering* of
  increments across threads is not modelled. A real counter under
  contention needs its own claim.
- **Persistence.** The Lean model has no notion of crash recovery.
  If a `Counter` is persisted to disk and the process crashes
  mid-flush, no claim is made about the post-recovery value. A
  real persistent counter needs a sibling claim that includes
  fsync ordering.
- **Overflow semantics.** Documented in §3.3.

## 6. Reviewer checklist

- [x] **[machine-checked]** `lake build` succeeds.
      *Evidence: `refine lean check EXAMPLE-002` → `Verified`.*
- [x] **[machine-checked]** `refine bundle verify
      artifacts/EXAMPLE-002` succeeds.
- [x] **[machine-checked]** Every Rust entity in §2 exists.
      *Evidence: `refine scan check EXAMPLE-002` → `Verified, 1/1
      types, 1/1 fns`.*
- [x] **[machine-checked]** Monotonicity test passes at the boundary.
      *Evidence: `incr_is_monotone_at_u64_max`.*
- [x] **[machine-checked]** The strict-increase boundary failure
      is documented by a test that pins down the actual behavior.
      *Evidence: `incr_does_not_strictly_increase_at_u64_max`.*
- [ ] **[needs human]** The deployment context is OK with the
      saturating-overflow choice (§3.3 option 1). If not, switch
      to `checked_incr` and require callers to handle `None`.

This is a tutorial doc; in a real project, "needs human" items
require an actual second engineer's review before the claim is
marketed as refined.
