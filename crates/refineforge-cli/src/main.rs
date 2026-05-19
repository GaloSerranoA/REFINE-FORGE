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

use refineforge_cli::{autonomous, bundle, claim, repair, runner, scaffold, scan};

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
    /// MVP autonomous driver: plans + executes a baseline
    /// workflow against the claim, escalating per the
    /// refineforge-escalation engine. See
    /// docs/autonomous-driver-plan.md.
    Autonomous {
        claim_id: String,
        /// Strategy (built-in: "mock"; "anthropic-mock"; "anthropic"
        /// — the last requires ANTHROPIC_API_KEY in the env)
        #[arg(long, default_value = "mock")]
        strategy: String,
        /// Cap cumulative API spend; fails closed when exceeded.
        /// Estimated $0.07/repair-attempt for `--strategy anthropic`.
        #[arg(long, default_value_t = 10.0)]
        max_cost_usd: f64,
        /// Operator identity (recorded in the run report's
        /// `operator` field; not used for any access control).
        #[arg(long)]
        operator: Option<String>,
        /// Plan and report without writing commits, packets,
        /// or the final RunReport JSON.
        #[arg(long)]
        dry_run: bool,
        /// If a LeanCheck step fails, dynamically inject a
        /// bounded Repair step (using `--strategy`) followed by
        /// a re-verifying LeanCheck. Capped at 2 attempts per
        /// run at the driver level. With `--strategy anthropic`
        /// this is where live LLM cost is incurred.
        #[arg(long)]
        auto_repair: bool,
        /// When a step Escalates, block-poll the operator's
        /// packet decision (Approved → continue, Rejected /
        /// EditAndResubmit / Partial → halt). Per criteria v0.3
        /// there is no timeout; operators run `refine
        /// escalations list` to see what's pending.
        #[arg(long)]
        await_decisions: bool,
        /// Plan §3 phase 4 bait: synthetically inject the
        /// counter-idealisation Action (u64 → Nat, UnsignedOverflow)
        /// into the plan. Drives EXAMPLE-002's Cat 2 escalation
        /// without requiring a live LLM to produce it.
        #[arg(long)]
        inject_counter_idealisation: bool,
        /// Append a Section 2 training step after BundleExport.
        /// Subprocess-shells to `refine-train run <path> --dry-run`
        /// (binary path overridable via `REFINEFORGE_REFINE_TRAIN_BIN`).
        /// Repeatable: each occurrence adds another step.
        #[arg(long = "inject-training")]
        inject_training: Vec<String>,
        /// Append a Section 4 bit-exact gate step after BundleExport
        /// (and after any training steps). Subprocess-shells to
        /// `refine-bitexact run <path>` (binary path overridable via
        /// `REFINEFORGE_REFINE_BITEXACT_BIN`). Repeatable.
        #[arg(long = "inject-bitexact")]
        inject_bitexact: Vec<String>,
    },
    /// `refine escalations list` — operator queue dashboard.
    /// Per criteria v0.3 the autonomous driver never
    /// auto-rejects; this command is how the operator sees
    /// what's pending.
    Escalations {
        #[command(subcommand)]
        cmd: EscalationsCmd,
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

#[derive(Subcommand)]
enum EscalationsCmd {
    /// List every escalation packet across the project,
    /// sorted by age, with PENDING / DECIDED status.
    List {
        /// Filter to a single claim id.
        #[arg(long)]
        claim: Option<String>,
        /// Only show packets older than N days.
        #[arg(long)]
        age_gt: Option<u32>,
    },
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
        Cmd::Autonomous {
            claim_id,
            strategy,
            max_cost_usd,
            operator,
            dry_run,
            auto_repair,
            await_decisions,
            inject_counter_idealisation,
            inject_training,
            inject_bitexact,
        } => autonomous::run_cli(
            &cli.root,
            &claim_id,
            &strategy,
            max_cost_usd,
            operator.as_deref(),
            dry_run,
            auto_repair,
            await_decisions,
            inject_counter_idealisation,
            &inject_training,
            &inject_bitexact,
        ),
        Cmd::Escalations { cmd } => match cmd {
            EscalationsCmd::List { claim, age_gt } => {
                autonomous::escalations_list(&cli.root, claim.as_deref(), age_gt)
            }
        },
        Cmd::New {
            template,
            claim_id,
            module,
            title,
        } => scaffold::create(&cli.root, &template, &claim_id, &module, title.as_deref()),
        Cmd::Templates => scaffold::list_templates(&cli.root),
    }
}
