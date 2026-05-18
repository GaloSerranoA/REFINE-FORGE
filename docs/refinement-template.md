# `<CLAIM-ID>` — Refinement argument

> **Status of this document:** TEMPLATE. The Rust implementation
> does not yet exist (or has not yet been linked to this claim).
> The human operator owning this claim MUST fill in the
> *Implementation* and *Mapping* sections below and obtain a peer
> review before the claim's `status:` field is changed from
> `proven` (model-only) to `proven` (model + refined).
>
> Copy this file to `docs/refinement/<CLAIM-ID>.md` and edit.
>
> The Lean side is the source of truth for the mathematical content.
> This document is the trust-critical bridge from that model to the
> Rust binary the project actually ships.

## 1. What the Lean model says

In `lean/<YourLib>/<Module>.lean` we define:

| Lean entity              | Kind         | Meaning                              |
|--------------------------|--------------|--------------------------------------|
| `<TypeName>`             | structure    | TODO                                 |
| `<funcName>`             | function     | TODO                                 |
| `<PredicateName>`        | inductive    | TODO                                 |
| `<theoremName>`          | theorem      | TODO                                 |

Document the model assumptions explicitly. If the model
**abstracts** any cryptographic primitive (e.g. SHA-256 as an
opaque deterministic function), say so here.

## 2. What the Rust must implement

| Rust entity     | Kind     | Path                                              | Lean counterpart |
|-----------------|----------|---------------------------------------------------|------------------|
| `<TypeName>`    | struct   | `crates/<crate>/src/<file>.rs:<line>`             | `<Lean type>`    |
| `fn <funcName>` | function | `crates/<crate>/src/<file>.rs:<line>`             | `<Lean fn>`      |

Run `refine scan check <CLAIM-ID>` to confirm each entity exists at
the cited path. The scan is a static name-presence check; behavioural
correspondence is argued in §3.

## 3. Mapping

For each Rust ↔ Lean pair, write 2–6 sentences justifying the
correspondence. State idealisations explicitly (e.g. "Lean uses
`Nat`; Rust uses `[u8; 32]`; the theorems do not depend on
bit-width arithmetic so this idealisation is safe for this claim").

### 3.1 `<TypeName>` ↔ `<Lean type>`

**Implemented at:** `crates/<crate>/src/<file>.rs:<line-range>`.

- Field-by-field correspondence: …
- Idealisations: …
- Visibility / invariant placement: …

### 3.2 `<funcName>` ↔ `<Lean fn>`

**Implemented at:** `crates/<crate>/src/<file>.rs:<line-range>`.

- Pre-condition: …
- Post-condition: …
- Edge cases: …

### 3.3 (Repeat for every entity in §2.)

## 4. Trusted code base

This claim, even fully refined, depends on:

1. **Lean's kernel** — small, well-reviewed, ~6000 LoC of OCaml/C++.
2. **The Lean compiler v4.29.1** — pinned in `lean-toolchain`.
3. **`rustc` and LLVM** — used to compile the Rust code.
4. **Any cryptographic primitives in use** — e.g. `sha2 = "0.10"`.
   Each should have its own sibling claim under `<PROJECT>-CRYPTO-*`.
5. **The Rust standard library** — for `Vec`, slice ops, etc.
6. **The OS and hardware** — beyond the scope of any practical claim.

We make NO claim that any of these is itself verified. We claim
that *conditional on these being correct*, the cited Rust code
satisfies the claim.

## 5. What this claim does NOT cover

- **Concurrent access.** The Lean model has no notion of time or
  threads. If the Rust API permits concurrent use, document the
  serialisation strategy here.
- **Persistence.** The Lean model says nothing about what happens
  if the process crashes mid-operation. Document the persistence
  strategy and any recovery procedure that re-establishes the
  model's invariant on restart.
- **Side channels.** Timing attacks, cache attacks, etc., are
  not modelled.
- **Resource exhaustion.** Lean has no notion of OOM; Rust does.

Add or remove items as appropriate for the claim.

## 6. Reviewer checklist

A human reviewer should be able to certify each of the following
before this claim is marketed as a claim about the running system.
**[machine-checked]** items are verified by the CLI; **[needs human]**
items require a person to read code and adjudicate.

- [ ] **[machine-checked]** `lake build` succeeds against the pinned
      `lean-toolchain`. *Evidence: `refine lean check <CLAIM-ID>` →
      `Verified sorries=0 admits=0 axioms=0`.*
- [ ] **[machine-checked]** `refine bundle verify
      artifacts/<CLAIM-ID>` succeeds.
- [ ] **[machine-checked]** Every Rust entity in §2 exists at the
      cited path. *Evidence: `refine scan check <CLAIM-ID>` →
      `Verified`.*
- [ ] **[needs human]** The Lean model is what the project's
      documentation claims it is.
- [ ] **[needs human]** The field/argument layout of each entity
      matches the description in §3.
- [ ] **[needs human]** The cryptographic primitives cited in §3
      match the ones audited in sibling `<PROJECT>-CRYPTO-*` claims.
- [ ] **[needs human]** The "What this claim does NOT cover" items
      in §5 are acceptable for the deployment context (or are
      addressed by sibling claims).

Once every box is checked AND a second engineer has independently
checked at least the **[needs human]** items above, the claim
YAML's `review:` section may be populated and the claim's `status:`
field may be upgraded to indicate full refinement.
