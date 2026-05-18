# Methodology

## The claim refineforge lets you make

> *Project X* formally verifies **mathematical models** of
> trust-critical properties and links those models to audited Rust
> implementation boundaries.

## The claim refineforge does **not** let you make

> "*Project X* formally verifies its Rust code."

That stronger claim is false for any system built on this framework,
because:

1. The Rust source compiles, through `rustc` and LLVM, to a binary.
   The framework does not verify either compiler. (No one in
   production does.)
2. Direct verification of Rust source is a research-grade activity
   (RustBelt, Verus, Aeneas, Creusot). It is not what this framework
   does.
3. Even if you used a Rust verifier, the bridge from Rust semantics
   to "behaviour of the running system on the customer's hardware"
   still involves the OS, allocator, hardware, and side channels.

## What the bridge actually is

For each claim `<PROJECT>-<AREA>-NNN` refineforge expects three
artifacts:

1. **Lean model**  (e.g. `lean/<YourLib>/<Module>.lean`)
   A definition of the trust-critical behaviour as a mathematical
   structure plus an inductive predicate or function. This is what
   Lean type-checks.

2. **Lean theorems**
   Properties of the model. These are what `lake build` verifies.
   They cannot contain `sorry`, `admit`, or non-core `axiom`
   declarations. The no-sorry gate enforces this.

3. **Refinement argument**  (e.g. `docs/refinement/<CLAIM-ID>.md`)
   A human-authored, human-reviewed document that explains, for each
   Rust type and function listed in the claim's `rust_source` block,
   why it implements the Lean model. The argument identifies:
   - which Rust struct corresponds to which Lean structure
   - which Rust function corresponds to which Lean function/predicate
   - where the boundary between trusted and untrusted code is
   - which Rust invariants are *not* covered by the model
   - which abstractions in the model are *idealisations* of the code
     (e.g. SHA-256 modelled as a deterministic `Nat → Nat`)

   See [`refinement-template.md`](refinement-template.md) for the
   skeleton.

The refinement argument is **the** trust-critical artifact. Lean
checks (1) and (2) automatically; (3) requires a competent reviewer.

## Sequencing of trust

A customer or funder verifying a refineforge-backed claim should
follow this chain, in order, and stop at the first link they cannot
personally verify:

1. Install the pinned `lean v4.29.1` (or whatever your
   `lean-toolchain` says).
2. Run `lake build` in the bundle's reconstructed Lean directory.
   If it succeeds, the theorems hold under Lean's logic.
3. Read the Lean model. Decide whether the structures, functions,
   and predicates capture what the project's documentation claims
   they capture.
4. Read the refinement argument and the cited Rust source. Decide
   whether the Rust matches the Lean.

Steps 1 and 2 are mechanical. Step 3 takes minutes. Step 4 takes
hours and is where the customer's trust is actually established.

The role of refineforge is to make steps 1–3 cheap so step 4 is the
only thing the reviewer has to spend serious effort on.

## What "no sorry" buys you

`sorry` is Lean's escape hatch — it accepts any goal as proved.
A Lean file containing `sorry` will still type-check; `lake build`
will succeed. The no-sorry policy gate scans source text BEFORE
running Lake, so a `sorry`-laden file is rejected even if Lean
itself would accept it.

The same gate covers `admit` (alias for `sorry`) and top-level
`axiom` declarations. Lean core ships some axioms (propositional
extensionality, choice, quotient soundness). Anything beyond that
in user code is forbidden by default. A claim that genuinely needs
a custom axiom can override `policy.no_axioms_beyond_lean_core` in
its YAML — but doing so should be a deliberate, reviewed decision.

## What this framework does NOT protect against

- A wrong Lean model. Lean will happily prove theorems about a
  model that does not describe your system. The refinement argument
  is the only defence.
- A correct model, mis-cited. The claim YAML lists Rust files; if
  those files are not the ones actually deployed, the bundle is
  worthless. CI must pin claim YAMLs to git commits of the Rust
  source they cite.
- Compromised toolchain. The Lean version is pinned in
  `lean-toolchain`. The framework does not verify Lean itself.
  (Lean's kernel is small and has been extensively reviewed; this
  is the standard trust assumption.)

## Bundled Lake dependencies (Mathlib, Std, etc.)

A claim that imports Mathlib (or any other Lake-managed package)
has an additional trust link: the bundle's verifier-side
`lake build` will resolve those packages from `lake-manifest.json`,
which pins each dependency to a specific git commit.

`refine bundle export` includes `lake-manifest.json` (when present)
alongside `lakefile.toml` and `lean-toolchain`. This means:

1. **A bundle for a Mathlib-using claim is reproducible** in the
   sense that two builds against the same `lake-manifest.json` see
   the same Mathlib commit. The hash of the Mathlib source is NOT
   in the manifest (Mathlib is fetched by the verifier from
   `github.com/leanprover-community/mathlib4`), so the trust chain
   extends to GitHub's content-addressed storage of that commit.

2. **A bundle does NOT include the Mathlib source itself.** Mathlib
   is tens of MB; bundling it would defeat the "small, auditable
   archive" property. The verifier MUST run `lake update` (or
   `lake build`, which triggers a fetch) before re-checking, and
   MUST trust that the pinned commit in `lake-manifest.json`
   resolves to the same Mathlib bytes today as at bundle-export time.

3. **Mitigations available to a paranoid verifier:**
   - Pre-mirror Mathlib at the pinned commit to a local cache; run
     `lake build` offline.
   - Compare the SHA-256 of the resolved Mathlib `.git/objects`
     against an out-of-band trusted record.
   - For air-gapped review: build a Mathlib `lake` package
     locally, vendor it into the bundle out-of-band.

4. **A bundle that has NO `lake-manifest.json`** (i.e. the project
   uses zero Lake dependencies, as our EXAMPLE-* claims do) carries
   no Mathlib-trust dependency; the bundle's manifest hash chain is
   complete for its claim.

Sibling `<PROJECT>-DEPS-*` claims may be written to formalise the
trust in specific Lake packages; that is out of scope for this
framework today.

## Failure modes to advertise honestly

If a customer asks "is X proven?" the answer template is:

> The mathematical model in `lean/<lib>/...` proves Y. The
> refinement argument in `docs/refinement/<CLAIM-ID>.md` argues that
> the Rust code in `crates/...` implements that model. Both
> artifacts are in the verification bundle. If you accept Lean's
> logic, accept our refinement argument, and trust the Rust
> compiler and the OS, then X holds.

That sentence is long on purpose. It is the honest length.
