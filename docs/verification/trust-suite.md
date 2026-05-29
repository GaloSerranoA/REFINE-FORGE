# Refine-Forge Trust Suite — human-reviewed dogfood claims

> Snapshot: 2026-05-29. Reviewer of record: **Galo Serrano Abad**.
> A single index of the seven `human-reviewed` claims that formally verify
> Refine-Forge's *own* trust-critical Rust. Each is a real Lean proof (core
> Lean 4.29.1, no `sorry`/`admit`/axiom) linked to specific Rust via a
> human-reviewed refinement document.

## What this is

These are **dogfood** claims: Refine-Forge using its own pipeline
(Lean proof → `scan` symbol link → refinement doc → signed bundle → human
review) to verify the functions that make the framework trustworthy. Every claim
here reached the top trust level, `human-reviewed`, the honest way — a real human
adjudicated the model↔Rust correspondence and its idealisations.

## The seven claims

| Claim | Lean module | Real Rust | Proves |
|---|---|---|---|
| [TRUST-001](../refinement/REFINEFORGE-TRUST-001.md) | `AgentTrust` | `agent/common.rs` `enforce_trust_ceiling`/`trust_rank` | enforced trust never exceeds the ceiling |
| [TRUST-002](../refinement/REFINEFORGE-TRUST-002.md) | `OperatorGate` | `agent/common.rs` `is_automated_operator` | rejects AI/bot/placeholder operator names |
| [TRUST-003](../refinement/REFINEFORGE-TRUST-003.md) | `ApprovalGate` | `agent/common.rs` `validate_human_approval` | ∘TRUST-002 ⇒ an AI can't be recorded as a human approver |
| [TRUST-004](../refinement/REFINEFORGE-TRUST-004.md) | `AggregateTrust` | `agent/mod.rs` `lowest_trust`/`lowest_trust_ceiling` | `run_all` never over-trusts its weakest agent |
| [TRUST-005](../refinement/REFINEFORGE-TRUST-005.md) | `BundleVerify` | `bundle.rs` `verify`/`verify_with_options` | bundle verify accepts iff every file hash matches |
| [TRUST-006](../refinement/REFINEFORGE-TRUST-006.md) | `EscalationGate` | `refineforge-escalation` `engine.rs` `decide` | driver never auto-proceeds on axiom/operator-set/unknown |
| [TRUST-007](../refinement/REFINEFORGE-TRUST-007.md) | `PolicyGate` | `sorry_gate.rs` `check` | a forbidden token under policy is always rejected |

All seven: `status: proven`, `scope: model+refined`, `review.human_operator:
Galo Serrano Abad` (2026-05-29). Per-theorem content grades and proof shapes are
in [proof-inventory.md](proof-inventory.md); the historical de-vacuuming of the
CRS proofs is in [proof-audit.md](proof-audit.md).

## What the suite establishes

Two interlocking guarantees about Refine-Forge's *models* of its own Rust:

- **The anti-self-approval chain (002 → 003 → 006).** The operator gate rejects
  automated names (002); the approval gate requires a real operator and so
  rejects an AI approver (003, `claude_cannot_approve`); and the autonomous
  driver can never auto-set `review.human_operator` — it must escalate to a
  person (006). Together: *no AI, including the assistant that wrote these
  proofs, can record itself as a human approver* anywhere in the stack.
- **Gate + trust integrity (001, 004, 005, 007).** The policy gate rejects
  forbidden tokens (007); the bundle verifier rejects any hash mismatch (005);
  trust enforcement never exceeds its ceiling (001); and the run-all aggregate
  never claims more than its weakest member (004).

## What the suite does NOT establish (read this)

- **Per-claim, not repo-wide.** Each claim is `human-reviewed` individually.
  `refine agent lean --target helyx` (all claims) is still `model-only`, dragged
  down by the model-only CRS/EXAMPLE/HELYX claims. The suite does not change
  that.
- **Models, not binaries.** Each claim proves a property of a Lean *model* and a
  human-reviewed argument that the Rust implements it. Standard methodology
  caveats apply (you trust Lean's kernel, the pinned compiler, `rustc`/LLVM, and
  the refinement arguments — not a verified binary). See
  [methodology.md](../methodology.md).
- **Documented idealisations.** Each claim carries one (in its §3/§5). The two
  load-bearing ones: TRUST-005 does **not** cover SHA-256 collision resistance
  (a colliding forgery would pass — that is `sha2`'s job); TRUST-007 does **not**
  cover the regex/comment-strip lexer (it proves the verdict *given* correct
  counts).
- **Dogfood, not HELYX.** These verify Refine-Forge's own infrastructure, not
  HELYX production code.
- **Lean agent only.** This suite raises the Lean agent's per-claim trust. The
  DevOps/Train/Kernel agents are unaffected (DevOps = `release-ready-local`;
  Train/Kernel = `measured-only`).

## Re-verify any claim

```bash
# Proof + policy gate + scan + lint, all claims:
./target/release/refine lean check-all
./target/release/refine scan check-all
./target/release/refine lint check-all

# One claim, end-to-end with the agent trust verdict:
./target/release/refine agent lean --mode check --target REFINEFORGE-TRUST-001
#   → status=passed  trust_level=human-reviewed  production_proof=human-reviewed

# Re-hash a claim's exported bundle:
./target/release/refine bundle verify artifacts/REFINEFORGE-TRUST-001
```

## Extending trust to the other agents

This suite is the Lean agent's frontier. Raising the **DevOps** agent to
`release-ready-ci`, and **Train**/**Kernel** above `measured-only`, requires
real-world evidence (a hosted CI/OIDC run; real training compute; a real GPU) —
**not** something the assistant can fabricate. The exact operator sequence for
each is in
[../agents/human-reviewed-closure-runbook.md](../agents/human-reviewed-closure-runbook.md)
(DevOps CI evidence is §1; see also
[../release/devops-production-proof.md](../release/devops-production-proof.md)).
