# refineforge

Autor: Galo Serrano Abad
NANTAR AI ROOBOTICS

A Lean 4 proof engineering + refinement-bundle framework for trust-critical Rust.

> **Doctrine:** LLM may propose. Lean must verify. Human operator must approve.

## Documentation map

Read in this order — each doc is short and points at the next.

| Doc | What it covers |
|---|---|
| [README.md](README.md) (this file) | What refineforge is, how to install, the CLI surface |
| [STRUCTURE.md](STRUCTURE.md) | Every file in the repo, what owns it, how the pieces connect |
| [CHANGELOG.md](CHANGELOG.md) | Version history, what shipped in each release |
| [docs/methodology.md](docs/methodology.md) | The honest framing: what refineforge claims, what it does NOT claim |
| [docs/no-sorry-policy.md](docs/no-sorry-policy.md) | What the policy gate catches and what it does not |
| [docs/refinement-template.md](docs/refinement-template.md) | Empty template for writing your own refinement-argument doc |
| [docs/refinement/EXAMPLE-002.md](docs/refinement/EXAMPLE-002.md) | A filled-in refinement doc you can read as the answer key |
| [docs/llm-repair-design.md](docs/llm-repair-design.md) | Architecture of the LLM repair loop + how to swap in a real strategy |
| [docs/HELYX-CASE-STUDY.md](docs/HELYX-CASE-STUDY.md) | Pointer to the external worked example (helyx-proofforge) |

## What this is

refineforge is a **project template + CLI** for teams that want
formally-verified mathematical models linked to a running Rust
codebase. You write a Lean model, prove your theorems, write a
human-reviewed refinement argument bridging the model to the Rust
code, and refineforge bundles everything into a SHA-256-sealed
artifact a third party can independently re-verify.

Fork this repo, replace the example claim with your own, and you have
a verification pipeline.

## What this is *not*

It is **not** "AI that proves your code automatically." It does not
verify the Rust binary, the Rust compiler, the OS, or the hardware.
It proves properties of **mathematical models** of trust-critical
behaviour and links those models to specific Rust source files via
**a refinement argument that a human operator writes and reviews**.

The refinement argument — not the proof — is the trust-critical
artifact. See [`docs/methodology.md`](docs/methodology.md).

## Tutorials shipped in this repo

Two tutorial claims ship pre-wired so `refine lean check-all` works
the moment you `cargo build --release`:

| Claim | What it demonstrates | Files |
|-------|----------------------|-------|
| **EXAMPLE-001** | The minimum path: Lean theorem → policy gate → bundle. No Rust. | [`lean/Refineforge/Example.lean`](lean/Refineforge/Example.lean), [`claims/example.yaml`](claims/example.yaml) |
| **EXAMPLE-002** | The full refinement pattern: Lean theorem + Rust crate + refinement-argument doc. `refine scan` reports `Verified`. | [`lean/Refineforge/Counter.lean`](lean/Refineforge/Counter.lean), [`crates/example-counter/`](crates/example-counter), [`claims/example-counter.yaml`](claims/example-counter.yaml), [`docs/refinement/EXAMPLE-002.md`](docs/refinement/EXAMPLE-002.md) |

EXAMPLE-002 deliberately includes a real Lean-vs-Rust idealisation
(unbounded `Nat` vs saturating `u64`) so the refinement doc has
something non-trivial to argue. Read it as the answer-key for what
[`docs/refinement-template.md`](docs/refinement-template.md) asks
you to write for your own claims.

## External worked example

The HELYX trust-claim project (separate repo `helyx-proofforge` —
see [`docs/HELYX-CASE-STUDY.md`](docs/HELYX-CASE-STUDY.md)) is the
production-shape consumer of this framework. It demonstrates the
same pattern as EXAMPLE-002 with two real claims (append-only
audit chain + capability subsumption) and walks through the
`docs/refinement/` doc structure at full scale.

## Repository layout

```
refineforge/
├── lean/                      # Lake project — your formal models
│   ├── lakefile.toml
│   ├── lean-toolchain         # pinned: leanprover/lean4:v4.29.1
│   ├── Refineforge.lean       # library root (rename to your project)
│   └── Refineforge/
│       ├── Example.lean       # EXAMPLE-001: Lean-only hello world
│       └── Counter.lean       # EXAMPLE-002: refined tutorial (Lean side)
├── claims/                    # claim registry (YAML, one file per claim)
│   ├── example.yaml           # EXAMPLE-001 wired to the hello-world
│   └── example-counter.yaml   # EXAMPLE-002 wired to Lean + Rust crate
├── crates/
│   ├── refineforge-cli/       # Rust CLI: `refine`
│   └── example-counter/       # EXAMPLE-002 Rust side (refines Counter.lean)
├── templates/                 # scaffolding for new claims
│   ├── append_chain/          # append-only linked chain with hash check
│   ├── capability/            # capability-based authorization
│   └── state_machine/         # state-machine transitions
├── artifacts/                 # exported verification bundles
├── docs/
│   ├── methodology.md         # how refineforge thinks about trust
│   ├── no-sorry-policy.md     # what the policy gate enforces
│   ├── refinement-template.md # generic template for refinement-argument docs
│   ├── refinement/
│   │   └── EXAMPLE-002.md     # filled-in refinement doc for the tutorial
│   └── HELYX-CASE-STUDY.md    # link to the original worked example
└── .github/workflows/ci.yml   # builds Lean + Rust on every push
```

## Quick start

```bash
# 1. Install elan (Lean toolchain manager) — one time
curl -sSf https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh | sh

# 2. Build the CLI
cargo build --release

# 3. Build the Lean library (downloads pinned Lean 4.29.1 on first run)
(cd lean && lake build)

# 4. List claims
./target/release/refine claims list

# 5. Verify every claim end-to-end (policy gate + lake build)
./target/release/refine lean check-all

# 6. Export an independently verifiable bundle
./target/release/refine bundle export EXAMPLE-001

# 7. Re-verify the bundle (re-hashes everything; does not re-run Lean)
./target/release/refine bundle verify artifacts/EXAMPLE-001

# 8. Scaffold your first real claim from a template
./target/release/refine templates
./target/release/refine new \
    --template state_machine \
    --module Refineforge.OrderState \
    --title "Order state machine respects allowed transitions" \
    MYPROJ-STATE-001
```

## Subcommand reference

| Command                                | What it does                                                            |
|----------------------------------------|-------------------------------------------------------------------------|
| `refine claims list`                   | List every claim in `claims/`                                           |
| `refine claims show <id>`              | Print a claim's YAML                                                    |
| `refine lean check <id>`               | Verify one claim (policy gate + `lake build`)                           |
| `refine lean check-all`                | Verify every claim                                                      |
| `refine scan check <id>`               | Confirm a claim's `rust_source` entities exist in the cited Rust file   |
| `refine scan check-all`                | Same, for every claim                                                   |
| `refine bundle export <id>`            | Bundle the sources + manifest + report                                  |
| `refine bundle verify <bundle-dir>`    | Re-hash every file in a bundle and confirm the manifest matches         |
| `refine repair <id>` (SKELETON)        | Bounded LLM repair loop against Lean's LSP server. Default strategy is `mock` (declines every proposal) — swap in an LLM strategy per [`docs/llm-repair-design.md`](docs/llm-repair-design.md) |
| `refine templates`                     | List scaffolding templates                                              |
| `refine new --template <t> --module <M> <ID>` | Scaffold a new claim from a template                             |

## Status enum (CLI output)

| status              | meaning                                                          |
|---------------------|------------------------------------------------------------------|
| `verified`          | policy gate passed AND `lake build` succeeded                    |
| `build_failed`      | Lean rejected the source                                         |
| `policy_violation`  | `sorry` / `admit` / non-core `axiom` found; build was not run    |
| `tooling_error`     | `lake` not installed or filesystem error                         |

Scan additionally reports `Verified` / `Partial` / `FileMissing` / `NoRustSource`.

## Customising for your project

1. Rename `lean/Refineforge.lean` → `lean/<YourLib>.lean` and the
   directory `lean/Refineforge/` → `lean/<YourLib>/`.
2. Update `lean/lakefile.toml`: change `defaultTargets` and
   `[[lean_lib]] name`. The scaffolder auto-detects the new library
   name from `defaultTargets`, so `refine new` keeps working.
3. Delete `claims/example.yaml` and `lean/<YourLib>/Example.lean`.
4. Use `refine new` to scaffold your real claims.

## Framework build plan

Where each thing currently lives:

| Component                                  | Status              |
|--------------------------------------------|---------------------|
| Lean runner CLI                            | ✅ implemented      |
| No-sorry policy gate                       | ✅ implemented      |
| Claim registry (YAML schema)               | ✅ implemented      |
| Verification bundle exporter + verifier    | ✅ implemented      |
| Proof template generator (`refine new`)    | ✅ implemented      |
| Rust source scan (name-presence check)     | ✅ implemented      |
| Refinement-argument template               | ✅ `docs/refinement-template.md` |
| LLM repair loop (LSP client)               | ⚠️ skeleton landed; `mock` strategy only — wire your own LLM per [`docs/llm-repair-design.md`](docs/llm-repair-design.md) |
| Syn-based scan (parse, not regex)          | not yet             |

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
