//! Multi-category overlap: a single action tripping two or
//! more categories must list all of them. The "primary" used
//! for the packet template is the most-restrictive
//! (most-specific) hit.

use refineforge_escalation::{Action, Category, Decision, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

// v0.3 deleted the previous `add_lake_package_trips_scope_plus_trust_base`
// test: AddLakePackage is now Cat 8 ONLY (Mathlib first-use is a
// trust-footprint extension, not a scope expansion). See
// cat08_trust_base.rs::add_lake_package_escalates_as_trust_base_only
// for the v0.3 single-category test.

// AddKernelDirectory trips Scope (new kernels/<X>/) + BitExactRegression
// (new un-baselined target). Primary should be BitExactRegression.
#[test]
fn add_kernel_directory_trips_scope_plus_bit_exact() {
    let act = Action::AddKernelDirectory {
        kernel_id: "rope_v2".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());

    let cats = d.categories();
    assert!(cats.contains(&Category::Scope));
    assert!(cats.contains(&Category::BitExactRegression));
    assert_eq!(d.primary_category(), Some(Category::BitExactRegression));
}

// Summary should mention both the primary and the secondaries
// so the operator immediately sees the multi-trip. Use
// AddKernelDirectory under v0.3 since it's the canonical
// remaining multi-trip case (Scope + BitExactRegression).
#[test]
fn summary_lists_secondary_categories() {
    let act = Action::AddKernelDirectory {
        kernel_id: "rope_v2".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    if let Decision::Escalate(r) = d {
        assert!(
            r.summary.contains("scope"),
            "summary missing 'scope': {}",
            r.summary
        );
        assert!(
            r.summary.contains("rope_v2"),
            "summary missing primary kernel id: {}",
            r.summary
        );
    } else {
        panic!("expected Escalate");
    }
}

// Single-category cases should NOT advertise multi-trip.
#[test]
fn single_category_summary_has_no_also_trips_suffix() {
    let act = Action::WriteAxiom {
        module: "Refineforge.Crypto".into(),
        axiom_name: "h".into(),
        statement: "True".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    if let Decision::Escalate(r) = d {
        assert!(
            !r.summary.contains("also trips"),
            "single-cat summary leaked 'also trips': {}",
            r.summary
        );
    } else {
        panic!("expected Escalate");
    }
}
