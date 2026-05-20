# CLAIM-CRS-004 Refinement Note

The Lean theorem `ethical_gate_non_bypass` verifies the model slice used by the
Consciousness-rs safety contract: an allowed action cannot be routed as allowed
unless the modeled gate result is also allowed.

The theorem is structural. It does not prove a complete ethics system.
