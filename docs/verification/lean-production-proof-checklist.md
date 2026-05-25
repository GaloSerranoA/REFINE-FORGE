# Lean Production Proof Checklist

The Lean agent may emit `human-reviewed` only when every checked item has a
committed artifact.

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
