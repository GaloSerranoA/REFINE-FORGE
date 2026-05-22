# refineforge-derive

`refineforge-derive` provides `#[derive(LeanModel)]`, a limited Rust proc macro that emits a Lean `structure` declaration as a Rust string constant.

Support label: `supported-documentation-aid`.

`LeanModel` generates a Lean structure declaration for review and refinement documentation. It does not prove the Rust implementation correct, does not generate theorems, and does not replace a human-reviewed refinement document.

## Current scope

- Supports named Rust structs without generics or lifetimes.
- Maps primitive integer types, `bool`, `String`, `&str`, `[u8; N]`, and `Vec<T>` to simple Lean types.
- Emits byte-stable field ordering based on Rust declaration order.
- Is used by `crates/example-counter`.

## Non-goals

- No proof generation.
- No automatic refinement argument.
- No support for enums, unions, tuple structs, arbitrary references, generics, or nested struct expansion in v1.
