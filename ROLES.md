# Roles

refineforge is organised around three engineering disciplines. Full
detail lives in [ARCHITECTURE.md](ARCHITECTURE.md); this file is the
short version a contributor reads before picking up an issue.

## The three roles

| Role | Owns | Stable interface to other roles |
|---|---|---|
| **Lean 4 Specialist** (highest priority, foundations) | `lean/`, `claims/`, `templates/`, `docs/methodology.md`, `docs/no-sorry-policy.md`, `docs/refinement-template.md`, `docs/refinement/`, and the core CLI modules: `claim.rs`, `runner.rs`, `sorry_gate.rs`, `bundle.rs`, `scaffold.rs`, `scan.rs` | (a) `RepairStrategy` trait surface; (b) bundle manifest schema (`bundle_schema: 1`) |
| **ML Training Engineer** (repair intelligence) | `crates/refineforge-cli/src/repair/`, the planned `crates/refineforge-strategies/`, `training/`, `models/`, `docs/repair-evaluation.md` | Consumes the `RepairStrategy` trait; hands a model artifact + strategy crate to the DevOps engineer for packaging |
| **Infrastructure / DevOps** (production surface) | `.github/workflows/`, planned `nix/` (or `bazel/`), `containers/`, `attestation/`, `release/`, `docs/security.md`, `docs/reproducible-build.md` | Wraps the bundle format with signatures; packages model artifacts into containers; promises CI never lies |

## What "ownership" means here

- The owner is the **first reviewer** of changes inside their paths.
- The owner is the **only person** who may declare the boundary
  interface broken — e.g., the Lean specialist is the only one who
  can bump `bundle_schema` from `1` to `2`.
- The owner is the **author of the relevant section in
  ARCHITECTURE.md** and is responsible for keeping it honest as
  scope shifts.

## What ownership does NOT mean

- It does not stop other roles from contributing inside the path.
  Cross-section PRs are normal; review is mandatory.
- It does not imply hiring. One person can wear all three hats. The
  boundary still holds because it's about *concerns*, not *people*.

## Sequencing

The architecture is explicit about priority order
([§ Sequencing](ARCHITECTURE.md)):

1. **Section 1 first**, until it's complete enough to be useful.
   (Already shipped at 0.1.0.)
2. **Section 3 phase 1 second** — multi-arch CI + verifier container.
   Cheapest credibility win.
3. **Section 2 phase 1 third** — `AnthropicStrategy` + eval harness.
   Validates the trait surface against a real provider.
4. Later phases interleave: signing, hermetic builds, mutation
   pipeline, fine-tuning.

> *"If all three sections start at once with one engineer, every
> section is 30 % done and nothing ships."* — ARCHITECTURE.md

## Mapping a task to a role

Use this table when triaging issues:

| Symptom | Likely owner |
|---|---|
| A theorem doesn't compile / a claim is over-specified | Lean Specialist |
| `refine bundle verify` reports a hash mismatch | Lean Specialist (bundle format) |
| `refine repair` proposes garbage / produces nothing | ML Engineer |
| CI is red on macOS but green on Linux | DevOps |
| A reviewer wants to verify the build is bit-identical | DevOps |
| A refinement-doc reviewer disputes an idealisation claim | Lean Specialist |
| Cost-per-repair-attempt is too high | ML Engineer |

## Cross-section change protocol

If a single change touches files owned by more than one role:

1. Identify the **primary owner** by which interface is being moved.
   Adding a new strategy = ML; changing the trait signature = Lean
   (because the trait is Section 1's stable interface).
2. Get review from **every** affected owner before merge.
3. Update [ARCHITECTURE.md](ARCHITECTURE.md) and
   [STRUCTURE.md](STRUCTURE.md) in the same PR if the boundary moved.
