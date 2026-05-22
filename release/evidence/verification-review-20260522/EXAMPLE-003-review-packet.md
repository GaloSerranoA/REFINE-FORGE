# EXAMPLE-003 Review Packet

> Snapshot date: 2026-05-22
> Status: blocked on real human identity and approval.

## Proposed transition

| Field | From | To |
|---|---|---|
| `claim_id` | `EXAMPLE-003` | `EXAMPLE-003` |
| `review.human_operator` | `null` | blocked - no real human identity was provided in this execution turn |
| `review.reviewed_on` | `null` | blocked |

## Files requiring review

- `claims/example-capability-revocation.yaml`
- `docs/refinement/EXAMPLE-003.md`
- `lean/Refineforge/CapabilityRevocation.lean`
- `crates/example-capability/src/capability.rs`

## Machine-checkable checklist

These items must be confirmed by the final verification gates before a human signs:

- `refine lean check EXAMPLE-003`
- `refine scan check EXAMPLE-003`
- `refine lint check EXAMPLE-003`
- `refine bundle verify artifacts/EXAMPLE-003`
- `cargo test -p example-capability`

## Human checklist

- Confirm the finite three-right domain is sufficient for the intended consumer.
- Confirm persistence and distributed revocation are intentionally out of scope.
- Confirm the Rust `Capability`, `authorizes`, and `revoke` mapping in `docs/refinement/EXAMPLE-003.md`.
- Confirm the signer is a real human operator, not Codex, Claude, an AI process, or a placeholder.

## Decision

No YAML review fields were changed. The correct state remains:

```yaml
review:
  human_operator: null
  reviewed_on: null
```

This is a deliberate blocker, not an omitted task. A future operator can approve this packet by providing a real human identity and review date.
