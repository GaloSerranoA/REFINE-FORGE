//! Category 2 — Idealisation. Rust→Lean mappings that lose
//! information. Mirrors the criteria-doc §Category 2 examples.

use refineforge_escalation::{Action, Category, Engine, LossKind, ProjectContext};

fn eng() -> Engine {
    Engine::new()
}

fn map(rust: &str, lean: &str, lossy: Vec<LossKind>) -> Action {
    Action::MapRustToLean {
        rust_type: rust.into(),
        lean_type: lean.into(),
        lossy_kinds: lossy,
    }
}

// ---------- Positive: unsigned overflow loss ----------

#[test]
fn u64_to_nat_escalates() {
    let d = eng()
        .decide(
            &map("u64", "Nat", vec![LossKind::UnsignedOverflow]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
    assert_eq!(d.primary_category(), Some(Category::Idealisation));
}

#[test]
fn u8_to_nat_escalates() {
    let d = eng()
        .decide(
            &map("u8", "Nat", vec![LossKind::UnsignedOverflow]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: signed bit-width loss ----------

#[test]
fn i32_to_int_escalates() {
    let d = eng()
        .decide(
            &map("i32", "Int", vec![LossKind::SignedBitWidth]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: fixed-width loss ----------

#[test]
fn byte_array_to_nat_escalates() {
    let d = eng()
        .decide(
            &map("[u8; 32]", "Nat", vec![LossKind::FixedWidth]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: concurrency loss ----------

#[test]
fn mutex_t_to_t_escalates() {
    let d = eng()
        .decide(
            &map("Mutex<Counter>", "Counter", vec![LossKind::Concurrency]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: Result → T (failure path lost) ----------

#[test]
fn result_t_e_to_t_escalates() {
    let d = eng()
        .decide(
            &map(
                "Result<u64, Err>",
                "Nat",
                vec![LossKind::FailurePath, LossKind::UnsignedOverflow],
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: Option → T (absence lost) ----------

#[test]
fn option_t_to_t_escalates() {
    let d = eng()
        .decide(
            &map(
                "Option<u64>",
                "Nat",
                vec![LossKind::Absence, LossKind::UnsignedOverflow],
            ),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Positive: float rounding ----------

#[test]
fn f64_to_real_escalates() {
    let d = eng()
        .decide(
            &map("f64", "Real", vec![LossKind::FloatRounding]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_escalate());
}

// ---------- Negative: String → String (no loss) ----------

#[test]
fn string_to_string_proceeds() {
    let d = eng()
        .decide(
            &map("String", "String", vec![]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: bool → Bool (no loss) ----------

#[test]
fn bool_to_bool_proceeds() {
    let d = eng()
        .decide(
            &map("bool", "Bool", vec![]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}

// ---------- Negative: same-shape struct ----------

#[test]
fn same_shape_struct_proceeds() {
    let d = eng()
        .decide(
            &map("Position { x: i64, y: i64 }", "Position", vec![]),
            &ProjectContext::test_default(),
        )
        .unwrap();
    assert!(d.is_proceed());
}
