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

use refineforge_cli::{
    agent, approval, autonomous, bundle, claim, lint, memory, production_proof, release, repair,
    runner, scaffold, scan, training_approval,
};

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
    /// Lint claim metadata and refinement docs before running the
    /// heavier Lean / bundle gates.
    Lint {
        #[command(subcommand)]
        cmd: LintCmd,
    },
    /// Release readiness, evidence, SBOM, provenance, and audit gates.
    Release {
        #[command(subcommand)]
        cmd: ReleaseCmd,
    },
    /// Central non-authoritative memory records for Refine-Forge agents.
    Memory {
        #[command(subcommand)]
        cmd: MemoryCmd,
    },
    /// Four HELYX-facing specialist agents backed by CLI evidence reports.
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// Verify a full production-proof evidence pack across all four agents.
    ProductionProof {
        #[command(subcommand)]
        cmd: ProductionProofCmd,
    },
    /// Prepare or finalize explicit human approvals for training promotion evidence.
    TrainingApproval {
        #[command(subcommand)]
        cmd: TrainingApprovalCmd,
    },
    /// Prepare or finalize explicit human approvals for role evidence packs.
    Approval {
        #[command(subcommand)]
        cmd: ApprovalCmd,
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
        /// Local fine-tuned weights/runtime directory for --strategy local-finetune
        #[arg(long)]
        weights_path: Option<PathBuf>,
        /// Don't write changes to disk
        #[arg(long)]
        dry_run: bool,
        /// Override the strategy's built-in prompt with one of the
        /// templates from the prompt-template library. Currently
        /// honoured by `--strategy anthropic`/`anthropic-mock`; other
        /// strategies ignore it. Must reference a template whose
        /// `expected_output_format` is `patch_json` (the existing
        /// patch parser requires it).
        #[arg(long)]
        prompt_template: Option<String>,
        /// Path to the prompt-template library JSON. Defaults to
        /// `training/prompt_templates/lean_proof_repair_v1.json`
        /// relative to the project root.
        #[arg(long)]
        prompt_template_library: Option<PathBuf>,
    },
    /// MVP autonomous driver: plans + executes a baseline
    /// workflow against the claim, escalating per the
    /// refineforge-escalation engine. See
    /// docs/plans/autonomous-driver-plan.md.
    Autonomous {
        claim_id: String,
        /// Strategy (built-in: "mock"; "anthropic-mock"; "anthropic"
        /// — the last requires ANTHROPIC_API_KEY in the env)
        #[arg(long, default_value = "mock")]
        strategy: String,
        /// Local fine-tuned weights/runtime directory for --strategy local-finetune
        #[arg(long)]
        weights_path: Option<PathBuf>,
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
enum LintCmd {
    /// Lint one claim
    Check { claim_id: String },
    /// Lint every claim in the registry
    CheckAll,
}

#[derive(Subcommand)]
enum ReleaseCmd {
    /// Run release readiness gates and write evidence artifacts.
    Ready {
        /// Semver version being prepared, e.g. 0.2.2.
        #[arg(long)]
        version: String,
        /// Evidence output directory.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Plan and write evidence without running expensive gates.
        #[arg(long)]
        dry_run: bool,
        /// Permit a dirty working tree and record it as skipped evidence.
        #[arg(long)]
        allow_dirty: bool,
        /// Do not build or smoke-test the Docker verifier image.
        #[arg(long)]
        skip_docker: bool,
        /// Do not require or verify signatures during local readiness.
        #[arg(long)]
        skip_signature: bool,
        /// Mark report as a CI run and use CI-specific evidence defaults.
        #[arg(long)]
        ci: bool,
    },
    /// Build a local/offline release proof pack from existing release-ready and verifier evidence.
    OfflineProof {
        /// Semver version being prepared, e.g. 0.2.2.
        #[arg(long)]
        version: String,
        /// Evidence output directory.
        #[arg(long)]
        evidence_dir: PathBuf,
        /// Existing `refine release ready` evidence directory.
        #[arg(long)]
        release_ready_dir: PathBuf,
        /// Existing offline signature artifact produced by a local key workflow.
        #[arg(long)]
        signature_file: PathBuf,
        /// Human-verifiable local key fingerprint for the offline signature.
        #[arg(long)]
        key_fingerprint: String,
        /// Existing verifier log showing the offline verification command passed.
        #[arg(long)]
        verifier_log: PathBuf,
    },
}

#[derive(Subcommand)]
enum MemoryCmd {
    /// Add one memory record to the JSONL store.
    Add {
        /// Store path. Defaults to .refineforge/memory/records.jsonl under --root.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Agent role: lean, devops, train, kernel, or run_all.
        #[arg(long)]
        agent: String,
        /// Target project or system, e.g. helyx, cogn8ty, or refine-forge.
        #[arg(long)]
        target: String,
        /// Memory kind: preference, citation, evidence_index, handoff, claim_note, or blocker.
        #[arg(long)]
        kind: String,
        /// Memory content.
        #[arg(long)]
        content: String,
        /// Optional source file to hash and cite.
        #[arg(long)]
        source_path: Option<PathBuf>,
    },
    /// List memory records as JSON.
    List {
        /// Store path. Defaults to .refineforge/memory/records.jsonl under --root.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Optional agent filter.
        #[arg(long)]
        agent: Option<String>,
        /// Optional target filter.
        #[arg(long)]
        target: Option<String>,
        /// Optional kind filter.
        #[arg(long)]
        kind: Option<String>,
    },
    /// Import newline-delimited memory JSON records, de-duplicating by id.
    Import {
        /// Source JSONL file.
        from: PathBuf,
        /// Store path. Defaults to .refineforge/memory/records.jsonl under --root.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Export the memory store as newline-delimited JSON.
    Export {
        /// Destination JSONL file.
        out: PathBuf,
        /// Store path. Defaults to .refineforge/memory/records.jsonl under --root.
        #[arg(long)]
        store: Option<PathBuf>,
    },
}

#[derive(Clone, clap::Args)]
struct AgentCliOptions {
    /// Agent mode: inspect is read-only, check runs gates, repair/execute may run heavier workflows.
    #[arg(long, value_enum, default_value_t = agent::AgentMode::Inspect)]
    mode: agent::AgentMode,
    /// Target claim, release version, experiment id, kernel gate, or `helyx`.
    #[arg(long, default_value = "helyx")]
    target: String,
    /// Evidence output directory.
    #[arg(long, default_value = "agent-reports/latest")]
    out: PathBuf,
    /// Emit the JSON report to stdout after writing evidence files.
    #[arg(long)]
    json: bool,
    /// Permit live/expensive gates instead of safe local dry-runs or skips.
    #[arg(long)]
    allow_expensive: bool,
}

impl AgentCliOptions {
    fn into_agent_options(self) -> agent::AgentOptions {
        agent::AgentOptions {
            mode: self.mode,
            target: self.target,
            out_dir: self.out,
            emit_json: self.json,
            allow_expensive: self.allow_expensive,
        }
    }
}

#[derive(Subcommand)]
enum AgentCmd {
    /// Lean 4 / verification specialist agent.
    Lean(AgentCliOptions),
    /// Release / infrastructure / DevOps specialist agent.
    Devops(AgentCliOptions),
    /// ML / training specialist agent.
    Train(AgentCliOptions),
    /// CUDA / GPU kernel specialist agent.
    Kernel(AgentCliOptions),
    /// Run all four agents and write a combined HELYX readiness dashboard.
    RunAll(AgentCliOptions),
}

#[derive(Subcommand)]
enum ProductionProofCmd {
    /// Validate a self-contained evidence pack and write a production-proof report.
    Verify {
        /// Evidence directory containing evidence.json and all declared artifacts.
        #[arg(long)]
        evidence_dir: PathBuf,
        /// Evidence output directory for summary.json and summary.md.
        #[arg(long, default_value = "production-proof/reports/latest")]
        out: PathBuf,
        /// Target project or system, e.g. helyx.
        #[arg(long, default_value = "helyx")]
        target: String,
        /// Emit the JSON report to stdout after writing evidence files.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum TrainingApprovalCmd {
    /// Validate evidence and write draft approval artifacts only.
    Draft(TrainingApprovalCliOptions),
    /// Write approvals/training.json after explicit human review.
    Approve(TrainingApprovalApproveCliOptions),
}

#[derive(Subcommand)]
enum ApprovalCmd {
    /// Validate role evidence and write draft approval artifacts only.
    Draft(ApprovalDraftCliOptions),
    /// Write approvals/<role>.json after explicit human review.
    Approve(ApprovalApproveCliOptions),
}

#[derive(Clone, clap::Args)]
struct ApprovalDraftCliOptions {
    /// Role being approved. If omitted, inferred from --review-request.
    #[arg(long, value_enum)]
    role: Option<approval::ApprovalRole>,
    /// Evidence directory containing role evidence and approvals/.
    #[arg(long)]
    evidence_dir: Option<PathBuf>,
    /// Existing review request JSON. Can provide role and evidence_dir.
    #[arg(long)]
    review_request: Option<PathBuf>,
    /// Agent JSON report for roles that require one.
    #[arg(long)]
    agent_report: Option<PathBuf>,
    /// Role-aware approval policy YAML. Defaults to approval-policy.yaml.
    #[arg(long)]
    policy: Option<PathBuf>,
    /// Named human operator allowed by the approval policy.
    #[arg(long)]
    operator: String,
    /// Emit a JSON command summary to stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, clap::Args)]
struct ApprovalApproveCliOptions {
    /// Role being approved. If omitted, inferred from --review-request.
    #[arg(long, value_enum)]
    role: Option<approval::ApprovalRole>,
    /// Evidence directory containing role evidence and approvals/.
    #[arg(long)]
    evidence_dir: Option<PathBuf>,
    /// Existing review request JSON. Can provide role and evidence_dir.
    #[arg(long)]
    review_request: Option<PathBuf>,
    /// Existing draft approval JSON to require before final approval.
    #[arg(long)]
    draft: Option<PathBuf>,
    /// Agent JSON report for roles that require one.
    #[arg(long)]
    agent_report: Option<PathBuf>,
    /// Role-aware approval policy YAML. Defaults to approval-policy.yaml.
    #[arg(long)]
    policy: Option<PathBuf>,
    /// Named human operator allowed by the approval policy.
    #[arg(long)]
    operator: String,
    /// Required confirmation that the named human reviewed the evidence.
    #[arg(long)]
    i_reviewed_this_evidence: bool,
    /// Emit a JSON command summary to stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, clap::Args)]
struct TrainingApprovalCliOptions {
    /// Evidence directory containing training/ and approvals/ artifacts.
    #[arg(long)]
    evidence_dir: PathBuf,
    /// Training Agent JSON report. Defaults to <evidence-dir>/train-agent-report.stdout.json.
    #[arg(long)]
    agent_report: Option<PathBuf>,
    /// Approval policy YAML. Defaults to training/approval-policy.yaml.
    #[arg(long)]
    policy: Option<PathBuf>,
    /// Named human operator allowed by the approval policy.
    #[arg(long)]
    operator: String,
    /// Emit a JSON command summary to stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, clap::Args)]
struct TrainingApprovalApproveCliOptions {
    #[command(flatten)]
    common: TrainingApprovalCliOptions,
    /// Required confirmation that the named human reviewed the evidence.
    #[arg(long)]
    i_reviewed_this_evidence: bool,
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
        Cmd::Lint { cmd } => match cmd {
            LintCmd::Check { claim_id } => lint::lint_one(&cli.root, &claim_id),
            LintCmd::CheckAll => lint::lint_all(&cli.root),
        },
        Cmd::Release { cmd } => match cmd {
            ReleaseCmd::Ready {
                version,
                evidence_dir,
                dry_run,
                allow_dirty,
                skip_docker,
                skip_signature,
                ci,
            } => {
                let dir = evidence_dir.unwrap_or_else(|| {
                    PathBuf::from("release")
                        .join("evidence")
                        .join(format!("local-{version}"))
                });
                release::ready(
                    &cli.root,
                    release::ReleaseReadyOptions {
                        version,
                        evidence_dir: dir,
                        dry_run,
                        allow_dirty,
                        skip_docker,
                        skip_signature,
                        ci,
                    },
                )
            }
            ReleaseCmd::OfflineProof {
                version,
                evidence_dir,
                release_ready_dir,
                signature_file,
                key_fingerprint,
                verifier_log,
            } => release::offline_proof(
                &cli.root,
                release::OfflineProofOptions {
                    version,
                    evidence_dir,
                    release_ready_dir,
                    signature_file,
                    key_fingerprint,
                    verifier_log,
                },
            ),
        },
        Cmd::Memory { cmd } => match cmd {
            MemoryCmd::Add {
                store,
                agent,
                target,
                kind,
                content,
                source_path,
            } => {
                let record = memory::add_with_store(
                    &cli.root,
                    memory::AddOptions {
                        store,
                        agent,
                        target,
                        kind,
                        content,
                        source_path,
                    },
                )?;
                memory::print_json(&record)
            }
            MemoryCmd::List {
                store,
                agent,
                target,
                kind,
            } => {
                let records = memory::list(
                    &cli.root,
                    memory::ListOptions {
                        store,
                        agent,
                        target,
                        kind,
                    },
                )?;
                memory::print_json(&records)
            }
            MemoryCmd::Import { from, store } => {
                let summary = memory::import_jsonl(&cli.root, &from, store.as_ref())?;
                memory::print_json(&summary)
            }
            MemoryCmd::Export { out, store } => {
                let summary = memory::export_jsonl(&cli.root, &out, store.as_ref())?;
                memory::print_json(&summary)
            }
        },
        Cmd::Agent { cmd } => match cmd {
            AgentCmd::Lean(opts) => {
                agent::run_role(&cli.root, agent::AgentRole::Lean, opts.into_agent_options())
            }
            AgentCmd::Devops(opts) => agent::run_role(
                &cli.root,
                agent::AgentRole::Devops,
                opts.into_agent_options(),
            ),
            AgentCmd::Train(opts) => agent::run_role(
                &cli.root,
                agent::AgentRole::Train,
                opts.into_agent_options(),
            ),
            AgentCmd::Kernel(opts) => agent::run_role(
                &cli.root,
                agent::AgentRole::Kernel,
                opts.into_agent_options(),
            ),
            AgentCmd::RunAll(opts) => agent::run_all(&cli.root, opts.into_agent_options()),
        },
        Cmd::ProductionProof { cmd } => match cmd {
            ProductionProofCmd::Verify {
                evidence_dir,
                out,
                target,
                json,
            } => production_proof::verify(
                &cli.root,
                production_proof::VerifyOptions {
                    evidence_dir,
                    out_dir: out,
                    target,
                    emit_json: json,
                },
            ),
        },
        Cmd::TrainingApproval { cmd } => match cmd {
            TrainingApprovalCmd::Draft(opts) => training_approval::draft(
                &cli.root,
                training_approval::DraftOptions {
                    evidence_dir: opts.evidence_dir,
                    agent_report: opts.agent_report,
                    policy: opts.policy,
                    operator: opts.operator,
                    emit_json: opts.json,
                },
            ),
            TrainingApprovalCmd::Approve(opts) => {
                let common = opts.common;
                training_approval::approve(
                    &cli.root,
                    training_approval::ApproveOptions {
                        evidence_dir: common.evidence_dir,
                        agent_report: common.agent_report,
                        policy: common.policy,
                        operator: common.operator,
                        i_reviewed_this_evidence: opts.i_reviewed_this_evidence,
                        emit_json: common.json,
                    },
                )
            }
        },
        Cmd::Approval { cmd } => match cmd {
            ApprovalCmd::Draft(opts) => approval::draft(
                &cli.root,
                approval::DraftOptions {
                    role: opts.role,
                    evidence_dir: opts.evidence_dir,
                    review_request: opts.review_request,
                    agent_report: opts.agent_report,
                    policy: opts.policy,
                    operator: opts.operator,
                    emit_json: opts.json,
                },
            ),
            ApprovalCmd::Approve(opts) => approval::approve(
                &cli.root,
                approval::ApproveOptions {
                    role: opts.role,
                    evidence_dir: opts.evidence_dir,
                    review_request: opts.review_request,
                    draft: opts.draft,
                    agent_report: opts.agent_report,
                    policy: opts.policy,
                    operator: opts.operator,
                    i_reviewed_this_evidence: opts.i_reviewed_this_evidence,
                    emit_json: opts.json,
                },
            ),
        },
        Cmd::Repair {
            claim_id,
            max_iterations,
            strategy,
            weights_path,
            dry_run,
            prompt_template,
            prompt_template_library,
        } => repair::run_cli(
            &cli.root,
            &claim_id,
            max_iterations,
            &strategy,
            weights_path.as_deref(),
            dry_run,
            prompt_template.as_deref(),
            prompt_template_library.as_deref(),
        ),
        Cmd::Autonomous {
            claim_id,
            strategy,
            weights_path,
            max_cost_usd,
            operator,
            dry_run,
            auto_repair,
            await_decisions,
            inject_counter_idealisation,
            inject_training,
            inject_bitexact,
        } => autonomous::run_cli(autonomous::RunCliOptions {
            root: &cli.root,
            claim_id: &claim_id,
            strategy: &strategy,
            weights_path: weights_path.as_deref(),
            max_cost_usd,
            operator: operator.as_deref(),
            dry_run,
            auto_repair,
            await_decisions,
            inject_counter_idealisation,
            inject_training: &inject_training,
            inject_bitexact: &inject_bitexact,
        }),
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
