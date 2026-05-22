# Lean Verification Track Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden refineforge's Lean/verification core and validate it on one production-shaped capability-revocation claim.

**Architecture:** Keep the existing claim YAML and bundle schema stable. Upgrade `refine scan` behind the current command surface, add a separate `refine lint` pre-Lean gate, extend `refine new` to emit refinement docs with template provenance, and add one repo-local claim that exercises Lean, Rust source scanning, linting, bundle export, and bundle verification.

**Tech Stack:** Rust 2021, `syn` for Rust parsing, existing `sha2`/`hex` hashing, Clap subcommands, Lean 4.29.1, Cargo workspace tests.

---

## File Structure

- Modify `ARCHITECTURE.md`, `ROLES.md`, `STRUCTURE.md`: make the four-section ownership model consistent.
- Modify `crates/refineforge-cli/Cargo.toml`: add `syn = { version = "2", features = ["full"] }`.
- Modify `crates/refineforge-cli/src/scan.rs`: replace regex-only matching with structured `syn` collection plus deterministic scan hashing.
- Create `crates/refineforge-cli/src/lint.rs`: claim linter built on claim loading, structured scan, and refinement-doc checks.
- Modify `crates/refineforge-cli/src/lib.rs`: export `lint`.
- Modify `crates/refineforge-cli/src/main.rs`: add `refine lint check` and `refine lint check-all`.
- Modify `crates/refineforge-cli/src/scaffold.rs`: write a refinement doc from a template and pass template provenance substitutions.
- Modify `templates/*/claim.yaml.tmpl` and `templates/*/lean.lean.tmpl`: add template name/version metadata.
- Add `templates/refinement.md.tmpl`: refinement-doc skeleton emitted by `refine new`.
- Modify `crates/refineforge-derive/src/lib.rs`: tighten docs around deterministic generated output.
- Modify `crates/example-counter/tests/counter.rs`: add derive macro coverage for supported composite fields.
- Create `crates/example-capability/Cargo.toml`, `crates/example-capability/src/lib.rs`, `crates/example-capability/src/capability.rs`, and `crates/example-capability/tests/capability.rs`: Rust side for the dogfood claim.
- Modify root `Cargo.toml`: add `crates/example-capability` to workspace members.
- Create `lean/Refineforge/CapabilityRevocation.lean`: Lean model and theorems.
- Modify `lean/Refineforge.lean`: import the new Lean module.
- Create `claims/example-capability-revocation.yaml`: claim registry entry.
- Create `docs/refinement/EXAMPLE-003.md`: production-shaped refinement argument.

---

### Task 1: Documentation Alignment

**Files:**
- Modify: `ARCHITECTURE.md`
- Modify: `ROLES.md`
- Modify: `STRUCTURE.md`

- [ ] **Step 1: Patch stale three-section wording**

Change `ARCHITECTURE.md` title to `# Architecture - Four Sections` and update the opening table so it lists Section 4 as `CUDA / GPU kernel engineer`.

Change `STRUCTURE.md` opening text from `Lean Specialist / ML Engineer / DevOps` to `Lean Specialist / ML Engineer / DevOps / CUDA Engineer`, and change `ARCHITECTURE.md # three-section structure` to `four-section structure`.

Change `ROLES.md` headings and sequencing text from "three roles" to "four roles".

- [ ] **Step 2: Verify docs diff**

Run: `git diff -- ARCHITECTURE.md ROLES.md STRUCTURE.md`

Expected: only wording/status alignment, no code changes.

- [ ] **Step 3: Commit**

Run:

```powershell
git add ARCHITECTURE.md ROLES.md STRUCTURE.md
git commit -m "docs: align verification roles with four sections"
```

---

### Task 2: Structured Scan and Deterministic Hashing

**Files:**
- Modify: `crates/refineforge-cli/Cargo.toml`
- Modify: `crates/refineforge-cli/src/scan.rs`

- [ ] **Step 1: Add failing scanner tests**

Add `#[cfg(test)] mod tests` to `scan.rs` with tests that create temporary repo roots and claims:

- `structured_scan_ignores_comments_and_strings`: a Rust file contains `struct Ghost` only inside a comment/string and the claim asks for `Ghost`; expected status is `Partial`.
- `structured_scan_finds_impl_methods_and_free_functions`: a Rust file has `pub struct Counter`, `impl Counter { pub fn new() -> Self }`, and `pub fn incr`; expected status is `Verified`.
- `scan_hash_is_stable_when_source_order_changes`: two claims cite the same symbols in different order; expected `scan_hash` values match.

Run: `cargo test -p refineforge-cli scan::tests -- --nocapture`

Expected before implementation: compile failure or failing assertions because `scan_hash` and structured scan do not exist.

- [ ] **Step 2: Add `syn` dependency**

Add this dependency to `crates/refineforge-cli/Cargo.toml`:

```toml
syn                     = { version = "2", features = ["full"] }
```

- [ ] **Step 3: Implement symbol collection**

In `scan.rs`, add a private `DiscoveredSymbols` type with sorted `types` and `functions` vectors. Implement:

```rust
fn discover_symbols(text: &str) -> Result<DiscoveredSymbols>
fn collect_item_symbols(file: &syn::File) -> DiscoveredSymbols
fn fallback_discover_symbols(text: &str) -> DiscoveredSymbols
```

Structured collection must include `ItemStruct`, `ItemEnum`, `ItemType`, `ItemTrait`, `ItemFn`, and `ImplItem::Fn`.

Fallback collection is only for parse failure and must preserve the existing regex behavior with an explicit warning stored on `ScanItem`.

- [ ] **Step 4: Add deterministic hash fields**

Add:

```rust
pub scan_hash: String
```

to `ScanReport`, and add to `ScanItem`:

```rust
pub discovered_types: Vec<String>,
pub discovered_functions: Vec<String>,
pub warnings: Vec<String>,
```

Compute the hash by feeding sorted, normalized report fields into `Sha256`:

- claim id
- item path
- file existence
- found/missing types
- found/missing functions
- discovered types/functions
- warnings

Do not include absolute paths or timestamps.

- [ ] **Step 5: Print scan hash**

In `scan_one`, print `scan_hash: <hex>` after status. In `scan_all`, append `hash=<first12>` to each summary line.

- [ ] **Step 6: Verify scanner tests**

Run: `cargo test -p refineforge-cli scan::tests -- --nocapture`

Expected: all scanner tests pass.

- [ ] **Step 7: Commit**

Run:

```powershell
git add crates\refineforge-cli\Cargo.toml crates\refineforge-cli\src\scan.rs Cargo.lock
git commit -m "feat(scan): add structured symbols and stable hash"
```

---

### Task 3: Claim Linter

**Files:**
- Create: `crates/refineforge-cli/src/lint.rs`
- Modify: `crates/refineforge-cli/src/lib.rs`
- Modify: `crates/refineforge-cli/src/main.rs`

- [ ] **Step 1: Add failing linter tests**

Create unit tests in `lint.rs`:

- `lint_flags_missing_rust_source_file`
- `lint_flags_refined_claim_without_refinement_doc`
- `lint_flags_refinement_doc_missing_required_sections`
- `lint_passes_example_counter_shape`

Each test creates a temporary repo root with `claims/`, `docs/refinement/`, `lean/`, and Rust source files as needed.

Run: `cargo test -p refineforge-cli lint::tests -- --nocapture`

Expected before implementation: compile failure because `lint` does not exist.

- [ ] **Step 2: Implement linter model**

Create:

```rust
pub enum LintSeverity { Error, Warning }
pub struct LintIssue { pub severity: LintSeverity, pub claim_id: String, pub message: String }
pub struct LintReport { pub claim_id: String, pub issues: Vec<LintIssue> }
pub fn lint_claim(root: &Path, claim_path: &Path, claim: &Claim) -> Result<LintReport>
pub fn lint_one(root: &Path, claim_id: &str) -> Result<()>
pub fn lint_all(root: &Path) -> Result<()>
```

Rules:

- missing Rust source path: error
- scan status `Partial` or `FileMissing`: error
- `status` containing `refined` or equal to `proven` with `rust_source` entries and no `docs/refinement/<CLAIM-ID>.md`: error
- refinement doc missing any of sections `## 1. What the Lean model says`, `## 2. What the Rust must implement`, `## 3. Mapping`, `## 4. Trusted code base`, `## 5. What this claim does NOT cover`, `## 6. Reviewer checklist`: warning
- claim cites a Rust symbol not present in the refinement doc: warning

- [ ] **Step 3: Wire CLI**

In `lib.rs`, add `pub mod lint;`.

In `main.rs`, import `lint`, add:

```rust
Lint { #[command(subcommand)] cmd: LintCmd }
```

and:

```rust
#[derive(Subcommand)]
enum LintCmd {
    Check { claim_id: String },
    CheckAll,
}
```

Dispatch to `lint::lint_one` and `lint::lint_all`.

- [ ] **Step 4: Verify linter tests and CLI help**

Run:

```powershell
cargo test -p refineforge-cli lint::tests -- --nocapture
cargo run -p refineforge-cli --bin refine -- lint check EXAMPLE-002
```

Expected: tests pass and EXAMPLE-002 lints without errors.

- [ ] **Step 5: Commit**

Run:

```powershell
git add crates\refineforge-cli\src\lint.rs crates\refineforge-cli\src\lib.rs crates\refineforge-cli\src\main.rs
git commit -m "feat(lint): add claim linter"
```

---

### Task 4: Template Provenance and Refinement Doc Scaffolding

**Files:**
- Modify: `crates/refineforge-cli/src/scaffold.rs`
- Modify: `templates/append_chain/claim.yaml.tmpl`
- Modify: `templates/capability/claim.yaml.tmpl`
- Modify: `templates/capability_with_revocation/claim.yaml.tmpl`
- Modify: `templates/linear_types/claim.yaml.tmpl`
- Modify: `templates/state_machine/claim.yaml.tmpl`
- Modify: `templates/*/lean.lean.tmpl`
- Create: `templates/refinement.md.tmpl`

- [ ] **Step 1: Add failing scaffold test**

Add a scaffold unit test that creates a minimal template directory with `lean.lean.tmpl`, `claim.yaml.tmpl`, and `refinement.md.tmpl`, runs `create`, and asserts:

- `docs/refinement/<CLAIM-ID>.md` exists
- claim YAML includes `template:`
- refinement doc includes `Template provenance`

Run: `cargo test -p refineforge-cli scaffold::tests -- --nocapture`

Expected before implementation: compile failure or assertion failure because no refinement doc is written.

- [ ] **Step 2: Extend substitutions**

In `scaffold.rs`, add substitutions:

- `{{TEMPLATE}}`
- `{{TEMPLATE_VERSION}}`
- `{{REFINEMENT_FILE}}`

Use template version `1` for all existing templates.

- [ ] **Step 3: Write refinement doc**

Read `templates/<name>/refinement.md.tmpl` if present, otherwise read `templates/refinement.md.tmpl`. Write it to `docs/refinement/<CLAIM-ID>.md`. Refuse to overwrite existing docs.

- [ ] **Step 4: Add provenance metadata to templates**

Add a top-level YAML block to each `claim.yaml.tmpl`:

```yaml
template:
  name: {{TEMPLATE}}
  version: {{TEMPLATE_VERSION}}
```

Add a Lean comment near the top of each `lean.lean.tmpl`:

```lean
Template provenance: {{TEMPLATE}} v{{TEMPLATE_VERSION}}
```

- [ ] **Step 5: Verify scaffold tests**

Run: `cargo test -p refineforge-cli scaffold::tests -- --nocapture`

Expected: scaffold tests pass.

- [ ] **Step 6: Commit**

Run:

```powershell
git add crates\refineforge-cli\src\scaffold.rs templates
git commit -m "feat(scaffold): add refinement docs and template provenance"
```

---

### Task 5: Derive Macro Contract Tests

**Files:**
- Modify: `crates/refineforge-derive/src/lib.rs`
- Modify: `crates/example-counter/tests/counter.rs`

- [ ] **Step 1: Add supported-shape test**

In `counter.rs`, add a local struct deriving `LeanModel` with fields:

```rust
#[derive(refineforge_derive::LeanModel)]
struct DeriveShape {
    count: usize,
    signed: i64,
    active: bool,
    label: String,
    bytes: [u8; 32],
    trail: Vec<u8>,
}
```

Assert exact generated output:

```text
structure DeriveShape where
  count : Nat
  signed : Int
  active : Bool
  label : String
  bytes : ByteArray
  trail : List Nat
```

- [ ] **Step 2: Tighten derive docs**

In `crates/refineforge-derive/src/lib.rs`, add a short "Determinism" section saying field order is Rust declaration order and generated strings are stable for the same input.

- [ ] **Step 3: Verify derive tests**

Run: `cargo test -p example-counter lean_model -- --nocapture`

Expected: derive contract tests pass.

- [ ] **Step 4: Commit**

Run:

```powershell
git add crates\refineforge-derive\src\lib.rs crates\example-counter\tests\counter.rs
git commit -m "test(derive): pin LeanModel generated shape"
```

---

### Task 6: Production-Shaped Capability Revocation Claim

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/example-capability/Cargo.toml`
- Create: `crates/example-capability/src/lib.rs`
- Create: `crates/example-capability/src/capability.rs`
- Create: `crates/example-capability/tests/capability.rs`
- Create: `lean/Refineforge/CapabilityRevocation.lean`
- Modify: `lean/Refineforge.lean`
- Create: `claims/example-capability-revocation.yaml`
- Create: `docs/refinement/EXAMPLE-003.md`

- [ ] **Step 1: Write Rust tests first**

Add tests proving:

- fresh capability with `Read` authorizes `Read`
- fresh capability with no `Write` does not authorize `Write`
- revoked capability authorizes nothing
- revocation is idempotent

Run: `cargo test -p example-capability`

Expected before implementation: package missing or compile failure.

- [ ] **Step 2: Implement Rust capability crate**

Implement the finite right enum without `LeanModel`, because the macro supports structs only:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Right { Read, Write, Admin }
```

Derive `LeanModel` only for:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, LeanModel)]
pub struct Capability {
    read: bool,
    write: bool,
    admin: bool,
    revoked: bool,
}
```

Add:

```rust
impl Capability {
    pub fn fresh(rights: &[Right]) -> Self
    pub fn revoke(self) -> Self
    pub fn is_revoked(&self) -> bool
}
pub fn authorizes(capability: &Capability, right: Right) -> bool
pub fn revoke(capability: Capability) -> Capability
```

- [ ] **Step 3: Write Lean model**

Create `Refineforge.CapabilityRevocation` with:

- `inductive Right | read | write | admin`
- `structure Capability where read : Bool; write : Bool; admin : Bool; revoked : Bool`
- `def holds`
- `def authorizes`
- `def revoke`
- theorem `revoked_authorizes_nothing`
- theorem `fresh_capability_authorizes_held_right`
- theorem `revoke_is_idempotent`

Use no imported Mathlib modules.

- [ ] **Step 4: Add claim YAML and refinement doc**

Claim id: `EXAMPLE-003`

Status: `proven`

Rust source path: `crates/example-capability/src/capability.rs`

Types: `Capability`

Functions: `authorizes`, `revoke`

Refinement doc must name the idealisation: Lean uses an inductive `Right`, Rust uses a finite enum; both have exactly read/write/admin variants.

- [ ] **Step 5: Verify dogfood claim**

Run:

```powershell
cargo test -p example-capability
cargo run -p refineforge-cli --bin refine -- lean check EXAMPLE-003
cargo run -p refineforge-cli --bin refine -- scan check EXAMPLE-003
cargo run -p refineforge-cli --bin refine -- lint check EXAMPLE-003
cargo run -p refineforge-cli --bin refine -- bundle export EXAMPLE-003
cargo run -p refineforge-cli --bin refine -- bundle verify artifacts/EXAMPLE-003
```

Expected: all commands pass.

- [ ] **Step 6: Commit**

Run:

```powershell
git add Cargo.toml Cargo.lock crates\example-capability lean\Refineforge.lean lean\Refineforge\CapabilityRevocation.lean claims\example-capability-revocation.yaml docs\refinement\EXAMPLE-003.md artifacts\EXAMPLE-003
git commit -m "feat: add capability revocation dogfood claim"
```

---

### Task 7: Final Verification

**Files:**
- All touched files

- [ ] **Step 1: Format**

Run: `cargo fmt --all`

Expected: command exits 0.

- [ ] **Step 2: Targeted Rust tests**

Run:

```powershell
cargo test -p refineforge-cli
cargo test -p refineforge-derive
cargo test -p example-counter
cargo test -p example-capability
```

Expected: all tests pass.

- [ ] **Step 3: Lean and claim gates**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- lean check-all
cargo run -p refineforge-cli --bin refine -- scan check-all
cargo run -p refineforge-cli --bin refine -- lint check-all
```

Expected: all commands pass.

- [ ] **Step 4: Bundle dogfood**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- bundle export EXAMPLE-003
cargo run -p refineforge-cli --bin refine -- bundle verify artifacts/EXAMPLE-003
```

Expected: export and verify pass.

- [ ] **Step 5: Final status**

Run:

```powershell
git status --short --branch
git log --oneline -8
```

Expected: branch has the task commits and no unrelated dirt remains.

## Self-Review

- Spec coverage: documentation alignment, deterministic scan hashing, claim linter, template provenance, derive macro contract, and production dogfood claim are each mapped to a task.
- Red-flag scan: this plan avoids open-ended implementation gaps; code-producing steps identify concrete symbols, commands, and expected outcomes.
- Type consistency: scan hash lives on `ScanReport`; linter consumes `scan_claim`; template provenance is metadata, not a claim-schema version bump.
