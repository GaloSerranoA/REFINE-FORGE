-- Building this library pulls in all of Mathlib (from the prebuilt cache),
-- confirming the verifier environment resolves `import Mathlib`. The proof-search
-- backend does NOT build this; it writes a candidate to `ProverCandidate.lean` and
-- runs `lake env lean ProverCandidate.lean`. This module just makes `lake build`
-- a one-command "is Mathlib ready?" check.
import Mathlib
