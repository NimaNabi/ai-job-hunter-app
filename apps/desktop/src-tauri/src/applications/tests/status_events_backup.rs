//! `applicationStatusEvents` bundle section (#1402): status history survives
//! a backup/restore, and a bundle without the section restores as before.

use super::{support::*, *};

/// `(from, to, note, source, confirmed)` — everything a restore must keep,
/// minus the device-local `event_id` and the `at` the caller compares apart.
fn shape(events: &[StatusEvent]) -> Vec<(String, String, String, String, bool)> {
    events
        .iter()
        .map(|e| {
            (
                e.from_status.clone(),
                e.to_status.clone(),
                e.note.clone(),
                e.source.clone(),
                e.confirmed,
            )
        })
        .collect()
}

#[test]
fn status_history_round_trips_through_the_bundle() {
    let (_dir, store) = open_store();
    let id = track(&store, "Acme", "Analyst");
    store
        .set_status(&id, ApplicationStatus::Screening, "recruiter call")
        .unwrap();
    store
        .set_status(&id, ApplicationStatus::Rejected, "")
        .unwrap();
    let before = store.events(&id);
    assert!(before.len() >= 3, "seed + two transitions expected");

    let apps = store.export();
    let events = ApplicationStatusEvents(&store).export();

    let (_dir2, store2) = open_store();
    store2.import(&apps).unwrap();
    let n = ApplicationStatusEvents(&store2).import(&events).unwrap();
    assert_eq!(n, before.len());

    let after = store2.events(&id);
    assert_eq!(shape(&after), shape(&before));
    assert_eq!(
        after.iter().map(|e| e.at).collect::<Vec<_>>(),
        before.iter().map(|e| e.at).collect::<Vec<_>>(),
    );
}

#[test]
fn a_bundle_without_the_section_still_seeds_one_imported_event() {
    let (_dir, store) = open_store();
    let id = track(&store, "Acme", "Analyst");
    store
        .set_status(&id, ApplicationStatus::Screening, "")
        .unwrap();

    let (_dir2, store2) = open_store();
    store2.import(&store.export()).unwrap();

    let after = store2.events(&id);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].note, "imported");
}

#[test]
fn an_event_for_an_unknown_application_is_skipped() {
    let (_dir, store) = open_store();
    let id = track(&store, "Acme", "Analyst");
    let mut events = ApplicationStatusEvents(&store).export();
    events.as_array_mut().unwrap().push(serde_json::json!({
        "applicationId": "does-not-exist",
        "fromStatus": "",
        "toStatus": "applied",
        "at": 1,
        "note": "",
        "source": "user",
        "confirmed": true,
    }));

    let (_dir2, store2) = open_store();
    store2.import(&store.export()).unwrap();
    let n = ApplicationStatusEvents(&store2).import(&events).unwrap();
    assert_eq!(n, store.events(&id).len());
    assert!(store2.events("does-not-exist").is_empty());
}

#[test]
fn a_malformed_section_is_rejected_before_anything_is_cleared() {
    let (_dir, store) = open_store();
    let id = track(&store, "Acme", "Analyst");
    let before = store.events(&id).len();

    let not_array = ApplicationStatusEvents(&store).import(&serde_json::json!({}));
    assert!(matches!(not_array, Err(AppError::Parse(_))));

    let bad_record =
        ApplicationStatusEvents(&store).import(&serde_json::json!([{ "applicationId": id }]));
    assert!(bad_record.is_err());

    assert_eq!(store.events(&id).len(), before, "history must be untouched");
}

#[test]
fn a_provisional_email_write_survives_the_round_trip() {
    let (_dir, store) = open_store();
    let id = track(&store, "Acme", "Analyst");
    let mut events = ApplicationStatusEvents(&store).try_export().unwrap();
    events.as_array_mut().unwrap().push(serde_json::json!({
        "applicationId": id,
        "fromStatus": "applied",
        "toStatus": "rejected",
        "at": 2,
        "note": "",
        "source": "email",
        "confirmed": false,
    }));

    let (_dir2, store2) = open_store();
    store2.import(&store.export()).unwrap();
    ApplicationStatusEvents(&store2).import(&events).unwrap();
    let restored = ApplicationStatusEvents(&store2).try_export().unwrap();

    let email: Vec<_> = restored
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["source"] == "email")
        .collect();
    assert_eq!(email.len(), 1);
    assert_eq!(email[0]["confirmed"], false);
    assert_eq!(email[0]["toStatus"], "rejected");
}
