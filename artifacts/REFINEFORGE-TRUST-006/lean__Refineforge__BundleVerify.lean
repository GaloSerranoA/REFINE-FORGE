/-
REFINEFORGE-TRUST-005 — Bundle verification accepts iff every file hash matches.

DOGFOOD model+refined claim about Refine-Forge's OWN bundle verifier. The Lean
model mirrors the hash-comparison core of `verify` / `verify_with_options` in
crates/refineforge-cli/src/bundle.rs: a bundle verifies iff every manifest
entry's recomputed SHA-256 equals the recorded hash (the Rust collects a
`mismatches` list and passes iff it is empty).

HONEST SCOPE. SHA-256 is idealised as an opaque hash (`String → Nat`); the model
proves the COMPARISON logic, NOT SHA-256's collision resistance — a forged file
whose hash collides with the manifest would still pass (that is SHA-256's job,
not this gate). The refinement doc (docs/refinement/REFINEFORGE-TRUST-005.md §5)
records this. NOT yet human-reviewed → agent trust = model-linked.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

namespace Refineforge.BundleVerify

/-- Verify a manifest against recomputed hashes: passes iff every entry's
    recomputed hash equals the recorded one. `recompute path` idealises
    "SHA-256 of the file at `path`". Mirrors `verify_with_options`, which
    accepts iff its `mismatches` list is empty. -/
def verifyEntries (entries : List (String × Nat)) (recompute : String → Nat) : Bool :=
  entries.all (fun e => recompute e.1 == e.2)

/-- T1 (soundness). If verification passes, every file's recomputed hash matches
    its manifest entry. -/
theorem verify_implies_all_match
    (entries : List (String × Nat)) (recompute : String → Nat)
    (h : verifyEntries entries recompute = true) :
    ∀ e ∈ entries, recompute e.1 = e.2 := by
  unfold verifyEntries at h
  rw [List.all_eq_true] at h
  intro e he
  simpa using h e he

/-- T2 (tamper detection). A single mismatched file makes verification fail. -/
theorem mismatch_implies_reject
    (entries : List (String × Nat)) (recompute : String → Nat)
    (e : String × Nat) (hmem : e ∈ entries) (hmis : recompute e.1 ≠ e.2) :
    verifyEntries entries recompute = false := by
  unfold verifyEntries
  cases hall : entries.all (fun e => recompute e.1 == e.2) with
  | false => rfl
  | true =>
      rw [List.all_eq_true] at hall
      have heq : recompute e.1 = e.2 := by simpa using hall e hmem
      exact absurd heq hmis

/-- T3 (concrete). A tampered file (recomputed hash differs from the manifest)
    is rejected. -/
theorem tamper_is_detected :
    verifyEntries [("f", 1)] (fun _ => 2) = false := by decide

end Refineforge.BundleVerify
