//! Rust-source scan: a static check that every entity a claim's
//! `rust_source:` block names actually exists in the cited file.
//!
//! This is the cheapest possible bridge between the Lean model and the
//! Rust impl: it does NOT verify behaviour, only that the names the
//! claim points at are real. A refinement argument (in
//! `docs/refinement/<claim>.md`) is still required to justify that
//! the named entities mean what the claim says they mean.
//!
//! Implementation: regex-based. Fast, dependency-light, false-positive
//! tolerant. If the field needs full parsing, swap in `syn`.

use anyhow::{anyhow, Context, Result};
use regex::Regex;
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
}

pub struct ScanItem {
    pub path: String,
    pub file_exists: bool,
    pub types_found: Vec<String>,
    pub types_missing: Vec<String>,
    pub functions_found: Vec<String>,
    pub functions_missing: Vec<String>,
}

fn type_regex(name: &str) -> Regex {
    // Matches: pub struct Name, pub(crate) enum Name, type Name = ...
    let pat = format!(
        r"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:struct|enum|type)\s+{}\b",
        regex::escape(name)
    );
    Regex::new(&pat).expect("static regex must compile")
}

fn fn_regex(name: &str) -> Regex {
    // Matches: pub fn name, pub(crate) async fn name, const fn name,
    // unsafe fn name. extern-with-explicit-ABI is not handled (rare in
    // public APIs and not worth the regex-escaping cost).
    let pat = format!(
        r"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+|extern\s+)*fn\s+{}\b",
        regex::escape(name)
    );
    Regex::new(&pat).expect("static regex must compile")
}

pub fn scan_claim(root: &Path, c: &Claim) -> Result<ScanReport> {
    if c.rust_source.is_empty() {
        return Ok(ScanReport {
            claim_id: c.claim_id.clone(),
            status: ScanStatus::NoRustSource,
            items: Vec::new(),
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
            });
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;

        let mut types_found = Vec::new();
        let mut types_missing = Vec::new();
        for t in &src.types {
            if type_regex(t).is_match(&text) {
                types_found.push(t.clone());
            } else {
                types_missing.push(t.clone());
                any_entity_missing = true;
            }
        }

        let mut functions_found = Vec::new();
        let mut functions_missing = Vec::new();
        for f in &src.functions {
            if fn_regex(f).is_match(&text) {
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
        });
    }

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
            "{:<22} {:<14} {}",
            report.claim_id,
            report.status.to_string(),
            counts
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
    for it in &r.items {
        println!("  path: {}", it.path);
        println!("    file_exists: {}", it.file_exists);
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
