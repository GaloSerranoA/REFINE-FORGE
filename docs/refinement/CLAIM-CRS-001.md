# CLAIM-CRS-001 Refinement Note

The Lean theorem `workspace_broadcast_complete` verifies the model slice used
by the Consciousness-rs global-workspace contract: a constructed ignition trace
has one corresponding broadcast event count.

Current outcome: **model-only**. The theorem is structural and discharged by
the model definition. It does not prove the Rust implementation, biological
consciousness, or runtime liveness outside the modeled trace.

Upgrade condition: a future `model+refined` version must identify the exact
Rust broadcast trace representation, cite the implementation files/functions,
and pass human review before any Rust source is used as proof evidence.
