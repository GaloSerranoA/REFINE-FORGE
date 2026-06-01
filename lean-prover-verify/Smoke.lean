-- Mathlib-free toolchain check. `lean Smoke.lean` (no lake, no Mathlib) must exit
-- 0 once elan has installed leanprover/lean4:v4.9.0-rc1. Validates the pinned
-- toolchain + the verifier plumbing (write file → run checker → exit code) before
-- the heavy Mathlib build.
theorem refineforge_smoke : 1 + 1 = 2 := by rfl
