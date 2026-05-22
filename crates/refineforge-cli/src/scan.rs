//! Rust-source scan: a static check that every entity a claim's
//! `rust_source:` block names actually exists in the cited file.
//!
//! This is the cheapest possible bridge between the Lean model and the
//! Rust impl: it does NOT verify behaviour, only that the names the
//! claim points at are real. A refinement argument (in
//! `docs/refinement/<claim>.md`) is still required to justify that
//! the named entities mean what the claim says they mean.
//!
//! Implementation: structured `syn` parse first, regex fallback only
//! for files that cannot be parsed as Rust.

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::claim::{self, Claim};

#[derive(Debug, PartialEq, Eq)]
pub enum ScanStatus {
    /// All named types and functions found in all rust_source files.
    Verified,
    /// File(s) exist, but some declared entities are missing.
    Partial,
    /// At least one rust_source path does not exist on disk.
    FileMissing,
    /// Claim has no rust_source block.
    NoRustSource,
}

impl std::fmt::Display for ScanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ScanStatus::Verified => "Verified",
            ScanStatus::Partial => "Partial",
            ScanStatus::FileMissing => "FileMissing",
            ScanStatus::NoRustSource => "NoRustSource",
        };
        f.write_str(s)
    }
}

pub struct ScanReport {
    pub claim_id: String,
    pub status: ScanStatus,
    pub items: Vec<ScanItem>,
    pub scan_hash: String,
}

pub struct ScanItem {
    pub path: String,
    pub file_exists: bool,
    pub types_found: Vec<String>,
    pub types_missing: Vec<String>,
    pub functions_found: Vec<String>,
    pub functions_missing: Vec<String>,
    pub discovered_types: Vec<String>,
    pub discovered_functions: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DiscoveredSymbols {
    types: Vec<String>,
    functions: Vec<String>,
}

fn fallback_type_regex() -> Regex {
    Regex::new(r"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:struct|enum|type|trait|union)\s+([A-Za-z_][A-Za-z0-9_]*)\b")
        .expect("static regex must compile")
}

fn fallback_fn_regex() -> Regex {
    Regex::new(r"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+|extern\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)\b")
        .expect("static regex must compile")
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn sorted(values: &[String]) -> Vec<String> {
    let mut out = values.to_vec();
    sort_dedup(&mut out);
    out
}

fn discover_symbols(text: &str) -> (DiscoveredSymbols, Vec<String>) {
    match syn::parse_file(text) {
        Ok(file) => (collect_item_symbols(&file), Vec::new()),
        Err(e) => {
            let mut warnings = vec![format!(
                "could not parse Rust with syn; used regex fallback: {e}"
            )];
            let symbols = fallback_discover_symbols(text);
            if symbols.types.is_empty() && symbols.functions.is_empty() {
                warnings.push("regex fallback discovered no symbols".into());
            }
            (symbols, warnings)
        }
    }
}

fn collect_item_symbols(file: &syn::File) -> DiscoveredSymbols {
    let mut symbols = DiscoveredSymbols::default();
    for item in &file.items {
        collect_from_item(item, &mut symbols);
    }
    sort_dedup(&mut symbols.types);
    sort_dedup(&mut symbols.functions);
    symbols
}

fn collect_from_item(item: &syn::Item, symbols: &mut DiscoveredSymbols) {
    match item {
        syn::Item::Struct(item) => symbols.types.push(item.ident.to_string()),
        syn::Item::Enum(item) => symbols.types.push(item.ident.to_string()),
        syn::Item::Type(item) => symbols.types.push(item.ident.to_string()),
        syn::Item::Trait(item) => {
            symbols.types.push(item.ident.to_string());
            for trait_item in &item.items {
                if let syn::TraitItem::Fn(f) = trait_item {
                    symbols.functions.push(f.sig.ident.to_string());
                }
            }
        }
        syn::Item::Union(item) => symbols.types.push(item.ident.to_string()),
        syn::Item::Fn(item) => symbols.functions.push(item.sig.ident.to_string()),
        syn::Item::Impl(item) => {
            for impl_item in &item.items {
                if let syn::ImplItem::Fn(f) = impl_item {
                    symbols.functions.push(f.sig.ident.to_string());
                }
            }
        }
        syn::Item::Mod(item) => {
            if let Some((_, items)) = &item.content {
                for nested in items {
                    collect_from_item(nested, symbols);
                }
            }
        }
        _ => {}
    }
}

fn fallback_discover_symbols(text: &str) -> DiscoveredSymbols {
    let mut symbols = DiscoveredSymbols::default();
    for caps in fallback_type_regex().captures_iter(text) {
        symbols.types.push(caps[1].to_string());
    }
    for caps in fallback_fn_regex().captures_iter(text) {
        symbols.functions.push(caps[1].to_string());
    }
    sort_dedup(&mut symbols.types);
    sort_dedup(&mut symbols.functions);
    symbols
}

fn compute_scan_hash(claim_id: &str, items: &[ScanItem]) -> String {
    let mut h = Sha256::new();
    h.update(b"refineforge-scan-v1\n");
    h.update(claim_id.as_bytes());
    h.update(b"\n");
    for item in items {
        h.update(item.path.as_bytes());
        h.update(b"\n");
        h.update(if item.file_exists { b"1\n" } else { b"0\n" });
        hash_list(&mut h, "types_found", &item.types_found);
        hash_list(&mut h, "types_missing", &item.types_missing);
        hash_list(&mut h, "functions_found", &item.functions_found);
        hash_list(&mut h, "functions_missing", &item.functions_missing);
        hash_list(&mut h, "discovered_types", &item.discovered_types);
        hash_list(&mut h, "discovered_functions", &item.discovered_functions);
        hash_list(&mut h, "warnings", &item.warnings);
    }
    hex::encode(h.finalize())
}

fn hash_list(h: &mut Sha256, label: &str, values: &[String]) {
    h.update(label.as_bytes());
    h.update(b":");
    for value in values {
        h.update(value.as_bytes());
        h.update(b"\0");
    }
    h.update(b"\n");
}

pub fn scan_claim(root: &Path, c: &Claim) -> Result<ScanReport> {
    if c.rust_source.is_empty() {
        let items = Vec::new();
        let scan_hash = compute_scan_hash(&c.claim_id, &items);
        return Ok(ScanReport {
            claim_id: c.claim_id.clone(),
            status: ScanStatus::NoRustSource,
            items,
            scan_hash,
        });
    }

    let mut items = Vec::new();
    let mut any_file_missing = false;
    let mut any_entity_missing = false;

    for src in &c.rust_source {
        let path = root.join(&src.path);
        if !path.exists() {
            any_file_missing = true;
            items.push(ScanItem {
                path: src.path.clone(),
                file_exists: false,
                types_found: Vec::new(),
                types_missing: src.types.clone(),
                functions_found: Vec::new(),
                functions_missing: src.functions.clone(),
                discovered_types: Vec::new(),
                discovered_functions: Vec::new(),
                warnings: Vec::new(),
            });
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (symbols, warnings) = discover_symbols(&text);

        let mut types_found = Vec::new();
        let mut types_missing = Vec::new();
        for t in sorted(&src.types) {
            if symbols.types.binary_search(&t).is_ok() {
                types_found.push(t.clone());
            } else {
                types_missing.push(t.clone());
                any_entity_missing = true;
            }
        }

        let mut functions_found = Vec::new();
        let mut functions_missing = Vec::new();
        for f in sorted(&src.functions) {
            if symbols.functions.binary_search(&f).is_ok() {
                functions_found.push(f.clone());
            } else {
                functions_missing.push(f.clone());
                any_entity_missing = true;
            }
        }

        items.push(ScanItem {
            path: src.path.clone(),
            file_exists: true,
            types_found,
            types_missing,
            functions_found,
            functions_missing,
            discovered_types: symbols.types,
            discovered_functions: symbols.functions,
            warnings,
        });
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));

    let status = if any_file_missing {
        ScanStatus::FileMissing
    } else if any_entity_missing {
        ScanStatus::Partial
    } else {
        ScanStatus::Verified
    };

    Ok(ScanReport {
        claim_id: c.claim_id.clone(),
        status,
        scan_hash: compute_scan_hash(&c.claim_id, &items),
        items,
    })
}

pub fn scan_one(root: &Path, claim_id: &str) -> Result<()> {
    let (_, c) = claim::load(root, claim_id)?;
    let report = scan_claim(root, &c)?;
    print_report(&report);
    match report.status {
        ScanStatus::Verified | ScanStatus::NoRustSource => Ok(()),
        _ => Err(anyhow!("scan of {} did not pass", claim_id)),
    }
}

pub fn scan_all(root: &Path) -> Result<()> {
    let claims = claim::all(root)?;
    if claims.is_empty() {
        println!("(no claims found)");
        return Ok(());
    }
    let mut any_fail = false;
    for (_, c) in &claims {
        let report = scan_claim(root, c)?;
        let counts = summarise(&report);
        println!(
            "{:<22} {:<14} {}  hash={}",
            report.claim_id,
            report.status.to_string(),
            counts,
            &report.scan_hash[..12]
        );
        if matches!(report.status, ScanStatus::FileMissing | ScanStatus::Partial) {
            any_fail = true;
        }
    }
    if any_fail {
        Err(anyhow!("one or more claims failed source scan"))
    } else {
        Ok(())
    }
}

fn summarise(r: &ScanReport) -> String {
    let mut t_found = 0;
    let mut t_total = 0;
    let mut f_found = 0;
    let mut f_total = 0;
    for it in &r.items {
        t_found += it.types_found.len();
        t_total += it.types_found.len() + it.types_missing.len();
        f_found += it.functions_found.len();
        f_total += it.functions_found.len() + it.functions_missing.len();
    }
    format!("types={}/{}  fns={}/{}", t_found, t_total, f_found, f_total)
}

fn print_report(r: &ScanReport) {
    println!("claim: {}", r.claim_id);
    println!("status: {}", r.status);
    println!("scan_hash: {}", r.scan_hash);
    for it in &r.items {
        println!("  path: {}", it.path);
        println!("    file_exists: {}", it.file_exists);
        if !it.warnings.is_empty() {
            println!("    warnings:       {:?}", it.warnings);
        }
        if !it.discovered_types.is_empty() {
            println!("    discovered_types: {:?}", it.discovered_types);
        }
        if !it.discovered_functions.is_empty() {
            println!("    discovered_fns:   {:?}", it.discovered_functions);
        }
        if !it.types_found.is_empty() {
            println!("    types_found:    {:?}", it.types_found);
        }
        if !it.types_missing.is_empty() {
            println!("    types_MISSING:  {:?}", it.types_missing);
        }
        if !it.functions_found.is_empty() {
            println!("    fns_found:      {:?}", it.functions_found);
        }
        if !it.functions_missing.is_empty() {
            println!("    fns_MISSING:    {:?}", it.functions_missing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::{LeanInfo, Policy, RustSource};

    fn claim_with_source(types: Vec<&str>, functions: Vec<&str>) -> Claim {
        Claim {
            claim_id: "TEST-SCAN-001".into(),
            title: "scanner test".into(),
            description: String::new(),
            scope: "test".into(),
            status: "drafted".into(),
            authors: vec!["test".into()],
            rust_source: vec![RustSource {
                path: "src/lib.rs".into(),
                types: types.into_iter().map(str::to_string).collect(),
                functions: functions.into_iter().map(str::to_string).collect(),
            }],
            lean: LeanInfo {
                toolchain: "leanprover/lean4:v4.29.1".into(),
                module: "Refineforge.Test".into(),
                file: "lean/Refineforge/Test.lean".into(),
                theorems: vec![],
            },
            policy: Policy::default(),
        }
    }

    fn write_source(root: &Path, text: &str) {
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), text).unwrap();
    }

    #[test]
    fn structured_scan_ignores_comments_and_strings() {
        let td = tempfile::tempdir().unwrap();
        write_source(
            td.path(),
            r#"
            // pub struct Ghost;
            const S: &str = "pub fn haunt() {}";
            pub struct Real;
            "#,
        );
        let claim = claim_with_source(vec!["Ghost"], vec!["haunt"]);

        let report = scan_claim(td.path(), &claim).unwrap();

        assert_eq!(report.status, ScanStatus::Partial);
        let item = &report.items[0];
        assert_eq!(item.types_missing, vec!["Ghost"]);
        assert_eq!(item.functions_missing, vec!["haunt"]);
    }

    #[test]
    fn structured_scan_finds_impl_methods_and_free_functions() {
        let td = tempfile::tempdir().unwrap();
        write_source(
            td.path(),
            r#"
            pub struct Counter { value: u64 }

            impl Counter {
                pub fn new() -> Self { Self { value: 0 } }
            }

            pub fn incr(c: &Counter) -> Counter { Counter { value: c.value + 1 } }
            "#,
        );
        let claim = claim_with_source(vec!["Counter"], vec!["new", "incr"]);

        let report = scan_claim(td.path(), &claim).unwrap();

        assert_eq!(report.status, ScanStatus::Verified);
        let item = &report.items[0];
        assert_eq!(item.types_found, vec!["Counter"]);
        assert_eq!(item.functions_found, vec!["incr", "new"]);
        assert_eq!(item.discovered_types, vec!["Counter"]);
        assert_eq!(item.discovered_functions, vec!["incr", "new"]);
    }

    #[test]
    fn scan_hash_is_stable_when_source_order_changes() {
        let td = tempfile::tempdir().unwrap();
        write_source(
            td.path(),
            r#"
            pub fn beta() {}
            pub struct Alpha;
            pub fn alpha() {}
            pub struct Beta;
            "#,
        );
        let claim_a = claim_with_source(vec!["Alpha", "Beta"], vec!["alpha", "beta"]);
        let claim_b = claim_with_source(vec!["Beta", "Alpha"], vec!["beta", "alpha"]);

        let report_a = scan_claim(td.path(), &claim_a).unwrap();
        let report_b = scan_claim(td.path(), &claim_b).unwrap();

        assert_eq!(report_a.status, ScanStatus::Verified);
        assert_eq!(report_b.status, ScanStatus::Verified);
        assert_eq!(report_a.scan_hash, report_b.scan_hash);
        assert_eq!(report_a.scan_hash.len(), 64);
    }
}
