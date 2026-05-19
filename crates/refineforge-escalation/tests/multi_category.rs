//! Multi-category overlap: a single action tripping two or
//! more categories must list all of them. The "primary" used
//! for the packet template is the most-restrictive
//! (most-specific) hit.

use refineforge_escalation::{Action, Category, Decision, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

// AddLakePackage trips Scope (new external dep) + Trust-base
// (Lake-package surface). Primary should be TrustBaseExtension
// because its packet has the pin-version field Scope's lacks.
#[test]
fn add_lake_package_trips_scope_plus_trust_base() {
    let act = Action::AddLakePackage {
        name: "mathlib".into(),
        version_or_rev: "v4.29.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());

    let cats = d.categories();
    assert!(cats.contains(&Category::Scope), "missing Scope in {:?}", cats);
    assert!(
        cats.contains(&Category::TrustBaseExtension),
        "missing TrustBaseExtension in {:?}",
        cats
    );
    assert_eq!(d.primary_category(), Some(Category::TrustBaseExtension));
}

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
// so the operator immediately sees the multi-trip.
#[test]
fn summary_lists_secondary_categories() {
    let act = Action::AddLakePackage {
        name: "mathlib".into(),
        version_or_rev: "v4.29.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    if let Decision::Escalate(r) = d {
        assert!(
            r.summary.contains("scope"),
            "summary missing 'scope': {}",
            r.summary
        );
        assert!(
            r.summary.contains("Lake package"),
            "summary missing primary: {}",
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
