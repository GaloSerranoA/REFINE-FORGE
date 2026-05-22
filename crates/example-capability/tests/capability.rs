use example_capability::{authorizes, revoke, Capability, Right};

#[test]
fn fresh_capability_authorizes_held_right() {
    let capability = Capability::fresh(&[Right::Read]);

    assert!(authorizes(&capability, Right::Read));
}

#[test]
fn fresh_capability_rejects_missing_right() {
    let capability = Capability::fresh(&[Right::Read]);

    assert!(!authorizes(&capability, Right::Write));
}

#[test]
fn revoked_capability_authorizes_nothing() {
    let capability = Capability::fresh(&[Right::Read, Right::Write, Right::Admin]);
    let revoked = revoke(capability);

    assert!(!authorizes(&revoked, Right::Read));
    assert!(!authorizes(&revoked, Right::Write));
    assert!(!authorizes(&revoked, Right::Admin));
    assert!(revoked.is_revoked());
}

#[test]
fn revoke_is_idempotent() {
    let capability = Capability::fresh(&[Right::Read, Right::Admin]);

    assert_eq!(revoke(revoke(capability)), revoke(capability));
}
