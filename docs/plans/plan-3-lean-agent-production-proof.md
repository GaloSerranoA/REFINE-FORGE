# Plan 3 - Lean Agent Production Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` to implement this plan task-by-task. Do not use delegated agents unless the operator explicitly requests them in the current turn.

**Goal:** Raise the Lean 4 specialist agent from honest `model-only`/`model-linked` classification to the final enterprise trust classifier for implementation-backed claims.

**Architecture:** Keep `refine agent lean` as the proof authority and add a shared production-proof envelope to agent reports. The Lean agent becomes the final classifier only when Lean proofs, claim YAML, deterministic Rust scans, refinement documents, external memory citations, and human review evidence all agree.

**Tech Stack:** Rust, Lean 4, YAML claims, JSON schema, `refineforge-cli`, deterministic scan/lint gates, Markdown refinement docs.

---

## 1. Current Level

Current live level:

| Agent | Status | Trust |
|---|---|---|
| Lean | `passed` | `model-only` |

Meaning: the agent is alive, the report shape is valid, and the current claim set does not establish implementation proof. This is correct and must not be inflated.

## 2. Production-Proof Target

The Lean agent reaches enterprise production proof only when:

- `refine agent lean --mode execute --target all` passes.
- `trust_level = "human-reviewed"` only after explicit human review evidence exists.
- Every selected implementation claim has:
  - Lean theorem with no `sorry`, `admit`, or project-local axiom,
  - claim YAML with `scope: model+refined`,
  - deterministic Rust scan evidence for every cited symbol,
  - refinement document with required sections,
  - proof inventory row,
  - non-null `review.human_operator`,
  - signed or hashed bundle artifact.
- Model-only claims remain allowed, but the production-proof summary says they are excluded from implementation proof.

## 3. File Map

- Modify: `crates/refineforge-cli/src/agent/common.rs`
  - Add production-proof report envelope types.
- Modify: `crates/refineforge-cli/src/agent/lean.rs`
  - Derive Lean production proof from claims, scans, refinement docs, bundles, and review fields.
- Modify: `schemas/agent-report.schema.json`
  - Require the production-proof envelope.
- Modify: `crates/refineforge-cli/tests/agent_cli.rs`
  - Freeze Lean trust boundaries and production-proof gating.
- Modify: `docs/agents/lean-agent.md`
  - Document production-proof evidence.
- Modify: `docs/verification/proof-inventory.md`
  - Add production-proof columns.
- Create: `docs/verification/lean-production-proof-checklist.md`
  - Human reviewer checklist.

## 4. Work Breakdown

### Task 1 - Add Shared Production-Proof Envelope

**Files:**
- Modify: `crates/refineforge-cli/src/agent/common.rs`
- Modify: `schemas/agent-report.schema.json`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Write failing schema/report test**

Add this assertion helper to `crates/refineforge-cli/tests/agent_cli.rs`:

```rust
fn assert_production_proof_envelope(report: &serde_json::Value, expected_agent: &str) {
    let proof = &report["production_proof"];
    assert_eq!(proof["agent"], expected_agent);
    assert!(proof["profile"].as_str().unwrap().ends_with("-production-proof"));
    assert!(["blocked", "partial", "ready", "human-reviewed"]
        .contains(&proof["status"].as_str().unwrap()));
    assert_eq!(proof["trust_effect"], "bounded-by-evidence");
    assert!(proof["requirements"].as_array().unwrap().len() >= 4);
}
```

Call it from `assert_enterprise_report`.

- [ ] **Step 2: Run the focused failing test**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_lean_inspect_writes_report
```

Expected before implementation: failure because `production_proof` is missing.

- [ ] **Step 3: Add the Rust types**

Add to `crates/refineforge-cli/src/agent/common.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionProof {
    pub agent: AgentKind,
    pub profile: String,
    pub status: ProductionProofStatus,
    pub trust_effect: String,
    pub requirements: Vec<ProductionRequirement>,
    pub reviewer_evidence: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionProofStatus {
    Blocked,
    Partial,
    Ready,
    HumanReviewed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionRequirement {
    pub id: String,
    pub description: String,
    pub status: AgentStatus,
    pub evidence: Vec<String>,
}
```

Add `pub production_proof: ProductionProof` to `AgentReport`, initialized as blocked in `AgentReport::new`.

- [ ] **Step 4: Add schema requirement**

In `schemas/agent-report.schema.json`, add `production_proof` to the top-level `required` array and define:

```json
"production_proof": {
  "type": "object",
  "required": [
    "agent",
    "profile",
    "status",
    "trust_effect",
    "requirements",
    "reviewer_evidence",
    "blockers"
  ],
  "additionalProperties": false,
  "properties": {
    "agent": { "enum": ["lean", "devops", "train", "kernel", "run_all"] },
    "profile": { "type": "string", "minLength": 1 },
    "status": { "enum": ["blocked", "partial", "ready", "human-reviewed"] },
    "trust_effect": { "const": "bounded-by-evidence" },
    "requirements": { "type": "array", "minItems": 1 },
    "reviewer_evidence": { "type": "array", "items": { "type": "string" } },
    "blockers": { "type": "array", "items": { "type": "string" } }
  }
}
```

- [ ] **Step 5: Run the focused test**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_lean_inspect_writes_report
```

Expected: pass.

### Task 2 - Build Lean Production Classifier

**Files:**
- Modify: `crates/refineforge-cli/src/agent/lean.rs`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Add regression test for model-only not production-proof**

Add:

```rust
#[test]
fn agent_lean_model_only_claims_block_production_proof() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("lean-production");
    let output = run_refine(
        &["agent", "lean", "--mode", "check", "--target", "helyx"],
        &out,
    );
    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(report["trust_level"], "model-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(report["production_proof"]["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b.as_str().unwrap().contains("model-only")));
}
```

- [ ] **Step 2: Implement classifier requirements**

In `lean.rs`, after `claim_trust_floor`, add a helper that produces these requirement ids:

```text
lean.no_sorry_gate
lean.claim_scope_model_refined
lean.rust_scan_symbols
lean.refinement_docs
lean.bundle_hashes
lean.human_review
```

The helper must mark production proof `blocked` if any selected implementation claim has `scope: model-only`, missing refinement doc, missing Rust symbols, or `review.human_operator: null`.

- [ ] **Step 3: Keep trust capping strict**

Only set `TrustLevel::HumanReviewed` when production-proof status is `HumanReviewed`. If all machine evidence is ready but human review is absent, leave trust at `ModelLinked` or lower and set production status `Ready`.

- [ ] **Step 4: Run Lean agent tests**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_lean_model_only_claims_block_production_proof
cargo test -p refineforge-cli --test agent_cli agent_lean_check_keeps_model_only_scope_as_trust_floor
```

Expected: both pass.

### Task 3 - Add Reviewer Checklist

**Files:**
- Create: `docs/verification/lean-production-proof-checklist.md`
- Modify: `docs/agents/lean-agent.md`

- [ ] **Step 1: Create checklist**

Create:

```markdown
# Lean Production Proof Checklist

The Lean agent may emit `human-reviewed` only when every checked item has a committed artifact.

| Requirement | Evidence |
|---|---|
| Lean theorem builds | `refine lean check-all` command record |
| No `sorry` / `admit` / local axiom | claim policy and sorry gate output |
| Claim scope is `model+refined` | claim YAML |
| Rust symbols exist | deterministic scan report |
| Refinement doc exists | `docs/refinement/<CLAIM>.md` |
| Bundle hash exists | exported bundle manifest |
| Human review exists | non-null `review.human_operator` and dated notes |

Model-only claims are excluded from implementation production proof.
```

- [ ] **Step 2: Link from agent doc**

Add to `docs/agents/lean-agent.md`:

```markdown
For the production-proof human review gate, use `docs/verification/lean-production-proof-checklist.md`.
```

- [ ] **Step 3: Verify docs**

Run:

```powershell
Test-Path docs\verification\lean-production-proof-checklist.md
git diff --check
```

Expected: both succeed.

## 5. Acceptance Gate

Run:

```powershell
cargo clippy -p refineforge-cli --all-targets -- -D warnings
cargo test -p refineforge-cli --test agent_cli
cargo run -p refineforge-cli --bin refine -- --root . agent lean --mode check --target helyx --out agent-reports/lean-prod --json
```

Expected final state on today's repo: Lean passes, remains `model-only`, and reports production proof `blocked` until real model+refined claims and human review exist.
