# CLAIM-CRS-005 Refinement Note

The Lean theorem `phi_proxy_deterministic` verifies the model slice used by the
Consciousness-rs metric contract: Phi-proxy is deterministic for identical
inputs.

Current outcome: **model-only**. The theorem is a direct determinism fact
about the modeled proxy function. It does not prove the Rust implementation,
IIT Phi, integrated information, or phenomenal consciousness.

Upgrade condition: keep this model-only unless a precise Rust metric function
and input normalization boundary are identified and human-reviewed.
