# No-sorry policy

Every claim in `claims/` defaults to:

```yaml
policy:
  no_sorry: true
  no_admit: true
  no_axioms_beyond_lean_core: true
```

The runner enforces these BEFORE invoking `lake build`. If any check
fails, the claim's status becomes `policy_violation` and the build
is not run.

## Why

`sorry` and `admit` are Lean's "TODO" tokens. They turn any goal
into an accepted proof. A file containing `sorry` still type-checks
and `lake build` still succeeds. The policy gate is the difference
between "Lean accepted the file" and "we have a proof."

`axiom` declarations in user code introduce assumptions that Lean
cannot check. Lean core ships a small, well-reviewed set of axioms
(propositional extensionality, choice, quotient soundness). Adding
more in proof-bearing code silently widens the trust base.

## Overriding

A claim may legitimately need a custom axiom — e.g. modelling a
collision-resistant cryptographic hash as an opaque function with
an assumed property. To allow this, set in the claim YAML:

```yaml
policy:
  no_axioms_beyond_lean_core: false
```

Doing so is a deliberate, reviewed decision. The bundle's
`report.json` will list the axiom count, and the refinement
argument MUST justify each axiom.

## What the gate actually scans

- `\bsorry\b`  — word-boundary match, so `sorryNotSorry` is not caught.
- `\badmit\b`  — same.
- `^\s*axiom\b` — top-level `axiom` declaration only.

Lean comments (`-- ...` and nested `/- ... -/`) are stripped before
scanning, so a TODO note like `-- replace sorry below` does not
trip the gate. String literals are NOT stripped; a Lean source file
that contains the token `"sorry"` inside a string is still flagged
as suspicious for human review. A more precise lexer is a future
improvement, but for MVP the precision/recall tradeoff favours
"flag and let a human look."

## What the gate does NOT catch

- `theorem t : P := by exact (cast (by sorry) trivial)` if the
  `sorry` is in a nested term that the gate's comment-stripper
  doesn't reach. Currently the gate strips comments only; it does
  not parse Lean syntactically. This is acceptable because `sorry`
  in any form still appears as the literal token `sorry`.
- `theorem t : P := by have h : Q := by sorry; ...` — caught: the
  literal token is present.
- A theorem of the form `theorem t : True := trivial` followed by
  `theorem t' : False := t.elim ...` exploiting a kernel bug —
  not caught by ANY policy gate. This is in the trust base.
- Importing an upstream Lean library that internally uses `sorry`.
  The gate scans only this repo's source. A claim that imports
  Mathlib must trust Mathlib's own no-sorry CI.
