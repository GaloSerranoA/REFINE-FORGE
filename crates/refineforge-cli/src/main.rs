//! refineforge CLI.
//!
//! Doctrine: LLM may propose, Lean must verify, human operator must approve.
//! This binary is the "Lean must verify" enforcement point. It does not
//! generate proofs; it gates them.
//!
//! Modules live in `src/lib.rs` so external strategy crates can
//! import the same types this binary uses.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use refineforge_cli::{bundle, claim, repair, runner, scaffold, scan};

#[derive(Parser)]
#[command(
    name = "refine",
    version,
    about = "refineforge: Lean 4 proof engineering and refinement-bundle framework",
    long_about = "Manage trust claims, drive Lean verification, scan Rust source for \
                  cited entities, and produce independently checkable proof bundles. \
                  Lean is the source of truth; this tool only orchestrates."
)]
struct Cli {
    /// Path to repo root (default: current directory)
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Inspect claim registry
    Claims {
        #[command(subcommand)]
        cmd: ClaimsCmd,
    },
    /// Drive Lean verification
    Lean {
        #[command(subcommand)]
        cmd: LeanCmd,
    },
    /// Build and verify proof bundles
    Bundle {
        #[command(subcommand)]
        cmd: BundleCmd,
    },
    /// Scan claim `rust_source` blocks: check that each named type
    /// and function actually exists in the cited Rust file. Static
    /// name-presence check only — not a behavioural verification.
    Scan {
        #[command(subcommand)]
        cmd: ScanCmd,
    },
    /// SKELETON: bounded LLM repair loop. Spawns `lake env lean
    /// --server`, collects diagnostics, asks the strategy for
    /// patches, applies them, re-checks. The shipped `mock`
    /// strategy declines every diagnostic — swap it for an LLM
    /// strategy to make this useful. See docs/llm-repair-design.md.
    Repair {
        /// Claim id to repair
        claim_id: String,
        /// Maximum loop iterations
        #[arg(long, default_value_t = 5)]
        max_iterations: usize,
        /// Strategy name (built-in: "mock")
        #[arg(long, default_value = "mock")]
        strategy: String,
        /// Don't write changes to disk
        #[arg(long)]
        dry_run: bool,
    },
    /// Scaffold a new claim from a template
    New {
        /// Template name (run `refine templates` to list)
        #[arg(long)]
        template: String,
        /// Claim id, e.g. MYPROJ-AUTH-001
        claim_id: String,
        /// Lean module path, e.g. Refineforge.Auth (must start with your lean library namespace)
        #[arg(long)]
        module: String,
        /// Optional short title
        #[arg(long)]
        title: Option<String>,
    },
    /// List available scaffolding templates
    Templates,
}

#[derive(Subcommand)]
enum ClaimsCmd {
    /// List all claims
    List,
    /// Show one claim by id
    Show { claim_id: String },
}

#[derive(Subcommand)]
enum LeanCmd {
    /// Verify one claim
    Check { claim_id: String },
    /// Verify every claim in the registry
    CheckAll,
}

#[derive(Subcommand)]
enum BundleCmd {
    /// Export a verification bundle for one claim
    Export {
        claim_id: String,
        /// Output directory (default: artifacts/<claim-id>)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify a bundle's hashes (and optionally its Sigstore signature)
    Verify {
        bundle: PathBuf,
        /// Also verify the Sigstore signature alongside hashes.
        /// Requires `cosign` on PATH and `manifest.json.sigbundle`
        /// in the bundle directory. See docs/security.md §3.
        #[arg(long)]
        verify_signature: bool,
        /// Override the regex the signer's cert identity must match.
        /// Default: refineforge's canonical CI workflow identity.
        /// Also overridable via REFINEFORGE_EXPECTED_IDENTITY_REGEX.
        #[arg(long)]
        identity_regex: Option<String>,
        /// Override the OIDC issuer that issued the signer's cert.
        /// Default: GitHub Actions
        /// (https://token.actions.githubusercontent.com).
        /// Also overridable via REFINEFORGE_EXPECTED_OIDC_ISSUER.
        #[arg(long)]
        oidc_issuer: Option<String>,
    },
}

#[derive(Subcommand)]
enum ScanCmd {
    /// Scan rust_source for one claim
    Check { claim_id: String },
    /// Scan rust_source for every claim in the registry
    CheckAll,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Claims { cmd } => match cmd {
            ClaimsCmd::List => claim::list(&cli.root),
            ClaimsCmd::Show { claim_id } => claim::show(&cli.root, &claim_id),
        },
        Cmd::Lean { cmd } => match cmd {
            LeanCmd::Check { claim_id } => runner::check(&cli.root, &claim_id),
            LeanCmd::CheckAll => runner::check_all(&cli.root),
        },
        Cmd::Bundle { cmd } => match cmd {
            BundleCmd::Export { claim_id, out } => bundle::export(&cli.root, &claim_id, out),
            BundleCmd::Verify {
                bundle,
                verify_signature,
                identity_regex,
                oidc_issuer,
            } => bundle::verify_with_options(
                &bundle,
                &bundle::VerifyOptions {
                    verify_signature,
                    identity_regex,
                    oidc_issuer,
                },
            ),
        },
        Cmd::Scan { cmd } => match cmd {
            ScanCmd::Check { claim_id } => scan::scan_one(&cli.root, &claim_id),
            ScanCmd::CheckAll => scan::scan_all(&cli.root),
        },
        Cmd::Repair {
            claim_id,
            max_iterations,
            strategy,
            dry_run,
        } => repair::run_cli(&cli.root, &claim_id, max_iterations, &strategy, dry_run),
        Cmd::New {
            template,
            claim_id,
            module,
            title,
        } => scaffold::create(&cli.root, &template, &claim_id, &module, title.as_deref()),
        Cmd::Templates => scaffold::list_templates(&cli.root),
    }
}
