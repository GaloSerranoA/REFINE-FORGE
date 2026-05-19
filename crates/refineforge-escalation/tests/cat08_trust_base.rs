//! Category 8 — Trust-base extension. Toolchain / Lake / Cargo
//! / cosign / GHA-SHA / Docker-tool / Anthropic-model bumps.

use refineforge_escalation::{Action, Category, Decision, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

fn ctx_with_bundle_chain(crates: &[&str]) -> ProjectContext {
    let mut ctx = ProjectContext::test_default();
    for c in crates {
        ctx.bundle_chain_crates.insert((*c).into());
    }
    ctx
}

// ---------- Positive: lean-toolchain bump ----------

#[test]
fn bump_lean_toolchain_escalates() {
    let act = Action::BumpLeanToolchain {
        from: "leanprover/lean4:v4.29.1".into(),
        to: "leanprover/lean4:v4.30.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::TrustBaseExtension));
}

// ---------- Positive: add Lake package (v0.3 — Cat 8 only) ----------
//
// Under v0.3, Mathlib (and any Lake registry package) is a
// trust-footprint extension, NOT a scope expansion. The Cat 1
// trip on AddLakePackage that v0.2 produced was removed in v0.3.

#[test]
fn add_lake_package_escalates_as_trust_base_only() {
    let act = Action::AddLakePackage {
        name: "mathlib".into(),
        version_or_rev: "v4.29.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    let cats = d.categories();
    assert!(cats.contains(&Category::TrustBaseExtension));
    assert!(
        !cats.contains(&Category::Scope),
        "v0.3: AddLakePackage no longer trips Cat 1; got {:?}",
        cats
    );
    assert_eq!(d.primary_category(), Some(Category::TrustBaseExtension));
}

#[test]
fn add_lake_package_non_mathlib_also_trust_base_only() {
    let act = Action::AddLakePackage {
        name: "batteries".into(),
        version_or_rev: "v0.1.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    let cats = d.categories();
    assert_eq!(cats, vec![Category::TrustBaseExtension]);
}

#[test]
fn mathlib_import_after_package_already_in_manifest_proceeds() {
    // Per v0.3: the trust footprint is established when the
    // package enters lake-manifest. Subsequent per-module imports
    // do not re-trigger escalation.
    let mut ctx = ProjectContext::test_default();
    ctx.lake_packages_existing.insert("mathlib".into());
    let act = Action::AddLeanImport {
        module: "Refineforge.NewModule".into(),
        import_path: "Mathlib.Analysis.SpecialFunctions.Pow.Real".into(),
        is_mathlib: true,
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_proceed(), "v0.3: post-manifest Mathlib imports proceed; got {:?}", d);
}

// ---------- Positive: bump in-bundle Cargo pin ----------

#[test]
fn bump_in_bundle_cargo_pin_escalates() {
    let act = Action::BumpCargoPin {
        crate_name: "sha2".into(),
        from: "0.10.9".into(),
        to: "0.10.10".into(),
        in_bundle: true,
        cited_in_refinement_doc: false,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::TrustBaseExtension));
}

// ---------- Positive: switch crate (in bundle) ----------

#[test]
fn switch_in_bundle_crate_escalates() {
    let act = Action::SwitchCrate {
        from: "serde_yaml".into(),
        to: "yaml-rust".into(),
        in_bundle: true,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: bump cosign ----------

#[test]
fn bump_cosign_escalates() {
    let act = Action::BumpCosignVersion {
        from: "v2.4.1".into(),
        to: "v2.5.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: bump GH Action SHA ----------

#[test]
fn bump_gha_sha_escalates() {
    let act = Action::BumpGitHubActionSha {
        action: "sigstore/cosign-installer".into(),
        from: "abcd".into(),
        to: "ef01".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: add tool to verifier Dockerfile ----------

#[test]
fn add_verifier_docker_tool_escalates() {
    let act = Action::AddVerifierDockerTool {
        tool: "jq".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: switch ANTHROPIC_MODEL ----------

#[test]
fn switch_anthropic_model_escalates_when_unknown() {
    let act = Action::SwitchAnthropicModel {
        from: "claude-opus-4-7".into(),
        to: "claude-something-else-1".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: switch ANTHROPIC_MODEL when previously approved ----------

#[test]
fn switch_anthropic_model_proceeds_when_approved() {
    let mut ctx = ProjectContext::test_default();
    ctx.approved_anthropic_models
        .insert("claude-sonnet-4-6".into());
    let act = Action::SwitchAnthropicModel {
        from: "claude-opus-4-7".into(),
        to: "claude-sonnet-4-6".into(),
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: dev-dep update with no refinement-doc citation ----------

#[test]
fn dev_only_pin_with_no_doc_citation_proceeds() {
    let act = Action::BumpCargoPin {
        crate_name: "tempfile".into(),
        from: "3.10.0".into(),
        to: "3.11.0".into(),
        in_bundle: false,
        cited_in_refinement_doc: false,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: dev-dep with refinement-doc citation escalates ----------

#[test]
fn dev_dep_cited_in_refinement_doc_escalates() {
    let act = Action::BumpCargoPin {
        crate_name: "syn".into(),
        from: "2.0.0".into(),
        to: "2.1.0".into(),
        in_bundle: false,
        cited_in_refinement_doc: true,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Context-promoted: pin recognised via bundle_chain_crates ----------

#[test]
fn pin_promoted_via_context_bundle_chain_escalates() {
    // The Action says in_bundle: false, but the project context
    // says the crate IS in the bundle chain. Trust the context.
    let ctx = ctx_with_bundle_chain(&["regex"]);
    let act = Action::BumpCargoPin {
        crate_name: "regex".into(),
        from: "1.10.0".into(),
        to: "1.11.0".into(),
        in_bundle: false,
        cited_in_refinement_doc: false,
    };
    let d = eng().decide(&act, &ctx).unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: SwitchCrate when not in bundle ----------

#[test]
fn switch_non_bundle_crate_proceeds() {
    let act = Action::SwitchCrate {
        from: "tiny_http".into(),
        to: "axum-test".into(),
        in_bundle: false,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}
