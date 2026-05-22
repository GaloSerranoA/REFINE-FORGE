# CLAIM-CRS-004 Refinement Note

The Lean theorem `ethical_gate_non_bypass` verifies the model slice used by the
Consciousness-rs safety contract: an allowed action cannot be routed as allowed
unless the modeled gate result is also allowed.

Current outcome: **model-only**. The theorem is a structural routing invariant
over a modeled gate decision. It does not prove the Rust implementation, a
complete ethics system, policy correctness, or adversarial safety.

Upgrade condition: keep this model-only unless a precise Rust invariant is
identified and reviewed. Do not expand the claim into implementation assurance
by wording alone.
