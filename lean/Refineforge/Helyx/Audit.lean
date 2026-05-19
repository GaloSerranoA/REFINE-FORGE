/-
HELYX-AUDIT-001 — Audit-chain append-increments-length theorem.

REPRESENTATIVE SLICE of HELYX's `verified/lean/HELYX/Audit/Chain.lean`
+ `verified/lean/HELYX/Audit/Append.lean`, copied here under the
`Refineforge.Helyx.Audit` namespace so refineforge's lake project
can verify + bundle it.

The HELYX source-of-truth lives in the operator's separate
`C:\HELYX` repository under `verified/lean/HELYX/Audit/`. The
refinement doc at `docs/refinement/HELYX-AUDIT-001.md` documents
the bridge from this slice to HELYX's actual `helyx-audit-verified`
+ `helyx-audit` Rust crates.

This file is a verbatim copy of two HELYX modules; the namespace
is renamed for refineforge's lake (which owns `Refineforge.*`).
The two theorems are identical to HELYX's:

  - HELYX.Audit.Chain → Refineforge.Helyx.Audit.Chain
  - HELYX.Audit.Append → Refineforge.Helyx.Audit.append_increments_length

When HELYX's Lean spec evolves, this slice gets re-synced. The
operator's commit message should cite the HELYX commit that the
slice is current with.
-/

namespace Refineforge.Helyx.Audit

structure Chain where
  length : Nat
deriving Repr, DecidableEq

def empty : Chain := { length := 0 }

def append (chain : Chain) : Chain :=
  { length := chain.length + 1 }

def replay (chain : Chain) : Nat :=
  chain.length

def tampered (expected observed : Nat) : Bool :=
  expected != observed

/-- Appending to the bootstrap audit chain increases length by one.
    Verbatim copy of HELYX.Audit.append_increments_length. -/
theorem append_increments_length (chain : Chain) :
    (append chain).length = chain.length + 1 := by
  cases chain
  simp [append]

end Refineforge.Helyx.Audit
