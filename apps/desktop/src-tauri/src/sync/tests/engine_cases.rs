//! The two-device cases listed on #1402.

use super::*;

#[test]
fn two_devices_converge() {
    let server = FakeServer::new();
    let (a, b) = (Device::new(), Device::new());
    a.items.put("1", v("from a"));
    b.items.put("2", v("from b"));

    a.sync(&server, 10).unwrap();
    b.sync(&server, 20).unwrap();
    a.sync(&server, 30).unwrap();

    assert_eq!(a.items.snapshot(), b.items.snapshot());
    assert_eq!(a.items.snapshot().len(), 2);
}

#[test]
fn offline_edits_on_both_sides_end_the_same_with_the_later_sync_winning() {
    let server = FakeServer::new();
    let (a, b) = (Device::new(), Device::new());
    a.items.put("1", v("v0"));
    a.sync(&server, 10).unwrap();
    b.sync(&server, 20).unwrap();

    // Both edit record 1 while offline.
    a.items.put("1", v("edited on a"));
    b.items.put("1", v("edited on b"));

    a.sync(&server, 30).unwrap(); // a syncs first
    b.sync(&server, 40).unwrap(); // b syncs later: b wins
    a.sync(&server, 50).unwrap();

    assert_eq!(a.items.get("1"), Some(v("edited on b")));
    assert_eq!(b.items.get("1"), Some(v("edited on b")));
}

#[test]
fn equal_rev_conflicts_break_ties_by_device_id() {
    let server = FakeServer::new();
    let (a, b) = (Device::new(), Device::new());
    a.items.put("1", v("v0"));
    a.sync(&server, 10).unwrap();
    b.sync(&server, 20).unwrap();
    a.items.put("1", v("a"));
    b.items.put("1", v("b"));
    a.sync(&server, 30).unwrap();
    b.sync(&server, 30).unwrap();
    a.sync(&server, 40).unwrap();
    b.sync(&server, 50).unwrap();

    let winner = if a.state.device_id().unwrap() > b.state.device_id().unwrap() {
        v("a")
    } else {
        v("b")
    };
    assert_eq!(a.items.get("1"), Some(winner.clone()));
    assert_eq!(b.items.get("1"), Some(winner));
}

#[test]
fn a_delete_reaches_the_other_device_and_never_comes_back() {
    let server = FakeServer::new();
    let (a, b) = (Device::new(), Device::new());
    a.items.put("1", v("v0"));
    a.sync(&server, 10).unwrap();
    b.sync(&server, 20).unwrap();
    assert!(b.items.get("1").is_some());

    a.items.remove("1");
    a.sync(&server, 30).unwrap();
    b.sync(&server, 40).unwrap();
    assert_eq!(b.items.get("1"), None);

    // A third device that still holds an old copy pushes it as a fresh record
    // on its first sync (no baseline): the tombstone already on the server
    // wins when it pulls, and the copy never resurrects on a or b.
    let c = Device::new();
    c.items.put("1", v("stale copy"));
    c.sync(&server, 50).unwrap();
    a.sync(&server, 60).unwrap();
    b.sync(&server, 70).unwrap();
    assert_eq!(a.items.get("1"), None);
    assert_eq!(b.items.get("1"), None);
    assert_eq!(c.items.get("1"), None);
}

#[test]
fn a_first_sync_never_deletes_anything() {
    let server = FakeServer::new();
    let a = Device::new();
    a.items.put("1", v("keep me"));
    a.sync(&server, 10).unwrap();

    // A fresh install with nothing local must not tombstone a's records.
    let fresh = Device::new();
    fresh.sync(&server, 20).unwrap();
    a.sync(&server, 30).unwrap();
    assert_eq!(a.items.get("1"), Some(v("keep me")));
    assert_eq!(fresh.items.get("1"), Some(v("keep me")));
}

#[test]
fn a_fresh_install_adopts_settings_instead_of_overwriting_them_with_defaults() {
    let server = FakeServer::new();
    let a = Device::new();
    a.settings.put("settings", v("tuned on a"));
    a.sync(&server, 10).unwrap();

    let fresh = Device::new();
    fresh.settings.put("settings", v("defaults"));
    fresh.sync(&server, 20).unwrap();
    a.sync(&server, 30).unwrap();
    assert_eq!(fresh.settings.get("settings"), Some(v("tuned on a")));
    assert_eq!(a.settings.get("settings"), Some(v("tuned on a")));
}

#[test]
fn an_interrupted_sync_leaves_the_store_intact_and_resumes() {
    let server = FakeServer::new();
    let a = Device::new();
    for i in 0..5 {
        a.items.put(&i.to_string(), v("a"));
    }
    a.sync(&server, 10).unwrap();

    let b = Device::new();
    b.items.put("local", v("b only"));
    server.page_size.set(2);
    server.fail_pull_at.set(Some(2)); // dies after the first page
    assert!(b.sync(&server, 20).is_err());
    assert_eq!(b.items.get("local"), Some(v("b only")), "local data intact");
    assert_eq!(b.items.snapshot().len(), 3, "first page applied");

    server.fail_pull_at.set(None);
    b.sync(&server, 30).unwrap();
    a.sync(&server, 40).unwrap();
    assert_eq!(a.items.snapshot(), b.items.snapshot());
    assert_eq!(b.items.snapshot().len(), 6);
}

#[test]
fn a_failed_push_is_sent_again_next_time() {
    let server = FakeServer::new();
    let a = Device::new();
    a.items.put("1", v("x"));
    server.fail_push.set(true);
    assert!(a.sync(&server, 10).is_err());
    assert_eq!(server.len(), 0);
    server.fail_push.set(false);
    a.sync(&server, 20).unwrap();
    assert_eq!(server.len(), 1);
}

#[test]
fn a_malformed_page_is_rejected_before_anything_is_applied() {
    let server = FakeServer::new();
    let a = Device::new();
    a.items.put("1", v("x"));
    a.sync(&server, 10).unwrap();

    let b = Device::new();
    server.corrupt_next_page.set(true);
    assert!(b.sync(&server, 20).is_err());
    assert!(b.items.snapshot().is_empty());
    b.sync(&server, 30).unwrap();
    assert_eq!(b.items.get("1"), Some(v("x")));
}

#[test]
fn a_newer_schema_change_is_held_then_applied_after_an_update() {
    let server = FakeServer::new();
    let (a, b) = (Device::new(), Device::new());
    a.items.put("1", v("v0"));
    a.sync(&server, 10).unwrap();
    b.sync(&server, 20).unwrap();

    a.items.set_schema(2);
    a.items.put("1", v("new shape"));
    a.sync(&server, 30).unwrap();

    let report = b.sync(&server, 40).unwrap();
    assert_eq!(report.held, 1);
    assert_eq!(b.items.get("1"), Some(v("v0")), "held, not applied");

    b.items.set_schema(2); // the app was updated
    b.sync(&server, 50).unwrap();
    assert_eq!(b.items.get("1"), Some(v("new shape")));
}

#[test]
fn a_held_record_is_not_overwritten_by_a_local_edit() {
    let server = FakeServer::new();
    let (a, b) = (Device::new(), Device::new());
    a.items.put("1", v("v0"));
    a.sync(&server, 10).unwrap();
    b.sync(&server, 20).unwrap();
    a.items.set_schema(2);
    a.items.put("1", v("new shape"));
    a.sync(&server, 30).unwrap();
    b.sync(&server, 40).unwrap();

    // b edits the record while holding the newer-schema change: not pushed.
    b.items.put("1", v("old-schema edit"));
    let before = server.len();
    b.sync(&server, 50).unwrap();
    assert_eq!(server.len(), before);
    a.sync(&server, 60).unwrap();
    assert_eq!(a.items.get("1"), Some(v("new shape")));
}

#[test]
fn a_new_token_scope_starts_with_a_fresh_baseline() {
    let server_a = FakeServer::new();
    let server_b = FakeServer::new();
    let a = Device::new();
    a.items.put("1", v("x"));
    a.sync_scope(&server_a, 10, "scope-a").unwrap();
    // Same device, another namespace: everything is pushed again.
    a.sync_scope(&server_b, 20, "scope-b").unwrap();
    assert_eq!(server_b.len(), 1);
}

#[test]
fn a_synced_device_with_no_changes_pushes_nothing() {
    let server = FakeServer::new();
    let a = Device::new();
    a.items.put("1", v("x"));
    a.sync(&server, 10).unwrap();
    let n = server.len();
    a.sync(&server, 20).unwrap();
    a.sync(&server, 30).unwrap();
    assert_eq!(server.len(), n);
}
