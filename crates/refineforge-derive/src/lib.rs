//! `#[derive(LeanModel)]` — generate a Lean structure declaration
//! from a Rust struct.
//!
//! ## Usage
//!
//! ```ignore
//! use refineforge_derive::LeanModel;
//!
//! #[derive(LeanModel)]
//! pub struct Counter {
//!     value: u64,
//! }
//!
//! // Generated at compile time:
//! //   impl Counter {
//! //       pub const LEAN_MODEL: &'static str =
//! //           "structure Counter where\n  value : Nat";
//! //       pub fn lean_model() -> &'static str { Self::LEAN_MODEL }
//! //   }
//!
//! assert_eq!(Counter::LEAN_MODEL, "structure Counter where\n  value : Nat");
//! ```
//!
//! ## Supported field types
//!
//! | Rust type                                 | Lean type    | Notes                                    |
//! |-------------------------------------------|--------------|------------------------------------------|
//! | `u8`, `u16`, `u32`, `u64`, `usize`        | `Nat`        | Idealisation: Lean `Nat` is unbounded; Rust is finite. Refinement doc must address overflow. |
//! | `i8`, `i16`, `i32`, `i64`, `isize`        | `Int`        | Same idealisation re bounds.             |
//! | `bool`                                    | `Bool`       | Exact correspondence.                    |
//! | `String`, `&str`                          | `String`     | Lean `String` is UTF-8; Rust `String` is UTF-8. Direct mapping. |
//! | `[u8; N]`                                 | `ByteArray`  | The `N` size is NOT carried over — Lean `ByteArray` is length-variable. The refinement doc must note the fixed-length constraint. |
//! | `Vec<T>` (where `T` is one of the above)  | `List <T>`   | Direct mapping; Lean `List` is a singly-linked cons list. |
//!
//! ## Unsupported
//!
//! - Generics, lifetimes, traits on the struct itself
//! - Nested structs (would need to derive `LeanModel` on the inner
//!   too AND have the macro merge their declarations — out of scope
//!   for v1)
//! - Tuple structs (`struct Foo(u64, bool)`)
//! - Unit structs (`struct Foo;` is permitted; generates a `where`
//!   with no fields)
//! - Enums and unions
//!
//! When an unsupported type is encountered, the macro emits a
//! `syn::Error` pointing at the offending field — the compiler
//! shows a normal error message with file:line.
//!
//! ## Determinism
//!
//! Generated fields are emitted in Rust declaration order. For the
//! same input struct and field types, `LEAN_MODEL` is byte-stable
//! across runs so downstream scan/lint/bundle evidence does not
//! drift.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

#[proc_macro_derive(LeanModel)]
pub fn derive_lean_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();

    // Reject generics / lifetimes for v1. A future version could
    // monomorphise; for now, this is a clear error rather than
    // silent miscompile.
    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "LeanModel: generics and lifetimes are not supported in v1",
        )
        .to_compile_error()
        .into();
    }

    let data_struct = match &input.data {
        Data::Struct(s) => s,
        Data::Enum(e) => {
            return syn::Error::new_spanned(
                e.enum_token,
                "LeanModel: enums are not supported; only structs",
            )
            .to_compile_error()
            .into();
        }
        Data::Union(u) => {
            return syn::Error::new_spanned(
                u.union_token,
                "LeanModel: unions are not supported; only structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let model = match &data_struct.fields {
        Fields::Named(named) => {
            let mut field_lines = Vec::new();
            for field in &named.named {
                let fname = field.ident.as_ref().unwrap().to_string();
                match rust_type_to_lean(&field.ty) {
                    Ok(t) => field_lines.push(format!("  {fname} : {t}")),
                    Err(e) => return e.to_compile_error().into(),
                }
            }
            format!("structure {name_str} where\n{}", field_lines.join("\n"))
        }
        Fields::Unit => format!("structure {name_str} where"),
        Fields::Unnamed(unnamed) => {
            return syn::Error::new_spanned(
                unnamed,
                "LeanModel: tuple structs are not supported; use named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let expanded = quote! {
        impl #name {
            /// Lean structure declaration auto-generated from this
            /// Rust struct by `#[derive(LeanModel)]`.
            ///
            /// See `crates/refineforge-derive/src/lib.rs` for the
            /// Rust→Lean type mapping table. The generated string is
            /// the bare `structure` declaration; `deriving` clauses
            /// (Repr, DecidableEq, etc.) are NOT added — the human
            /// operator decides which to include in their actual
            /// Lean source.
            pub const LEAN_MODEL: &'static str = #model;

            /// Returns the Lean structure declaration for this type.
            #[inline]
            pub fn lean_model() -> &'static str {
                Self::LEAN_MODEL
            }
        }
    };

    expanded.into()
}

/// Map a Rust type expression to its Lean counterpart per the table
/// in this crate's module docs. Returns a `syn::Error` (with the
/// field's span) for unsupported types so the compiler error points
/// at the right line.
fn rust_type_to_lean(ty: &Type) -> Result<String, syn::Error> {
    match ty {
        Type::Path(tp) => {
            let last = tp
                .path
                .segments
                .last()
                .ok_or_else(|| syn::Error::new_spanned(ty, "LeanModel: empty type path"))?;
            let name = last.ident.to_string();
            match name.as_str() {
                "u8" | "u16" | "u32" | "u64" | "usize" => Ok("Nat".into()),
                "i8" | "i16" | "i32" | "i64" | "isize" => Ok("Int".into()),
                "bool" => Ok("Bool".into()),
                "String" => Ok("String".into()),
                "Vec" => {
                    // Need to extract the type parameter.
                    let inner = extract_first_generic_arg(&last.arguments)
                        .ok_or_else(|| {
                            syn::Error::new_spanned(
                                ty,
                                "LeanModel: Vec must have a single type parameter",
                            )
                        })?;
                    let inner_lean = rust_type_to_lean(inner)?;
                    // Lean's `List` syntax. Wrap in parens for any
                    // non-trivial inner type so precedence is unambiguous.
                    if inner_lean.contains(' ') {
                        Ok(format!("List ({})", inner_lean))
                    } else {
                        Ok(format!("List {}", inner_lean))
                    }
                }
                other => Err(syn::Error::new_spanned(
                    ty,
                    format!(
                        "LeanModel: unsupported field type `{other}` (supported: u*/i* ints, bool, String, &str, [u8; N], Vec<T>). See crates/refineforge-derive/src/lib.rs for the table."
                    ),
                )),
            }
        }
        Type::Reference(r) => {
            // &str → String (Lean has no borrow concept).
            if let Type::Path(tp) = &*r.elem {
                if let Some(last) = tp.path.segments.last() {
                    if last.ident == "str" {
                        return Ok("String".into());
                    }
                }
            }
            Err(syn::Error::new_spanned(
                ty,
                "LeanModel: only `&str` references are supported (not arbitrary borrows)",
            ))
        }
        Type::Array(arr) => {
            if let Type::Path(tp) = &*arr.elem {
                if let Some(last) = tp.path.segments.last() {
                    if last.ident == "u8" {
                        return Ok("ByteArray".into());
                    }
                }
            }
            Err(syn::Error::new_spanned(
                ty,
                "LeanModel: only `[u8; N]` arrays are supported (maps to Lean `ByteArray`)",
            ))
        }
        _ => Err(syn::Error::new_spanned(
            ty,
            "LeanModel: unsupported type shape (only paths, &str, [u8; N] in v1)",
        )),
    }
}

/// Pull the first type argument out of a `Vec<T>`-shaped path
/// argument. Returns None for malformed shapes.
fn extract_first_generic_arg(args: &syn::PathArguments) -> Option<&Type> {
    let syn::PathArguments::AngleBracketed(ab) = args else {
        return None;
    };
    for arg in &ab.args {
        if let syn::GenericArgument::Type(t) = arg {
            return Some(t);
        }
    }
    None
}
