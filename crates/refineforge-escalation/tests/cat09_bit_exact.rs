//! Category 9 — Bit-exact regression. Added in criteria v0.2.

use refineforge_escalation::{Action, Category, Engine, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

// ---------- Positive: edit kernel source ----------

#[test]
fn edit_kernel_source_escalates() {
    let act = Action::EditKernelSource {
        kernel_id: "matmul_fp32".into(),
        summary: "swap atomicAdd for warp-level reduction".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::BitExactRegression));
}

// ---------- Positive: change kernel build flags ----------

#[test]
fn change_kernel_build_flags_escalates() {
    let act = Action::ChangeKernelBuildFlags {
        kernel_id: "matmul_fp32".into(),
        from: "-arch=sm_80 -O3".into(),
        to: "-arch=sm_80 -O3 --use_fast_math".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: bump nvcc pin ----------

#[test]
fn bump_kernel_compiler_pin_escalates() {
    let act = Action::BumpKernelCompilerPin {
        kernel_id: "matmul_fp32".into(),
        compiler: "nvcc".into(),
        from: "12.3.0".into(),
        to: "12.4.0".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: lower run_count below baseline ----------

#[test]
fn lower_bitexact_run_count_escalates() {
    let act = Action::LowerBitExactRunCount {
        kernel_id: "matmul_fp32".into(),
        from: 5,
        to: 2,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: add a new kernel directory ----------

#[test]
fn add_kernel_directory_escalates() {
    let act = Action::AddKernelDirectory {
        kernel_id: "rope_v2".into(),
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_escalate());
    // multi-category: also trips Scope.
    let cats = d.categories();
    assert!(cats.contains(&Category::BitExactRegression));
    assert!(cats.contains(&Category::Scope));
}

// ---------- Negative: raise run_count above baseline ----------

#[test]
fn raise_run_count_proceeds() {
    let act = Action::LowerBitExactRunCount {
        kernel_id: "matmul_fp32".into(),
        from: 5,
        to: 10,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: same run_count proceeds ----------

#[test]
fn same_run_count_proceeds() {
    let act = Action::LowerBitExactRunCount {
        kernel_id: "matmul_fp32".into(),
        from: 5,
        to: 5,
    };
    let d = eng().decide(&act, &ProjectContext::test_default()).unwrap();
    assert!(d.is_proceed());
}
