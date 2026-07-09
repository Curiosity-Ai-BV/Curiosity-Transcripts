use std::fs;
use std::path::PathBuf;

use curiosity_app::{attach_calendar_event_context_command, CalendarEventAttachmentDto};
use curiosity_domain::Meeting;
use curiosity_store::Store;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-app-calendar-attachment-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

fn migrated_store_with_meeting(name: &str) -> Store {
    let root = test_root(name);
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");
    store
}

#[test]
fn attach_calendar_event_context_persists_safe_public_event() {
    let store = migrated_store_with_meeting("safe-public");
    let attached = attach_calendar_event_context_command(
        &store,
        "meeting-1",
        event("event-1", "Design Review"),
        false,
        2_000,
    )
    .expect("attach safe event");

    assert_eq!(attached.source, "AppleCalendar");
    assert_eq!(attached.event_id, "event-1");
    assert_eq!(attached.event_title, "Design Review");
    assert!(!attached.privacy_confirmed);
    assert_eq!(
        store
            .meeting_calendar_context("meeting-1")
            .expect("persisted context")
            .expect("attached context")
            .event_id,
        "event-1"
    );
}

#[test]
fn attach_calendar_event_context_requires_confirmation_for_unknown_privacy() {
    let store = migrated_store_with_meeting("unknown-privacy");
    let mut unknown = event("event-1", "Design Review");
    unknown.privacy = "Unknown".to_string();

    let error =
        attach_calendar_event_context_command(&store, "meeting-1", unknown.clone(), false, 2_000)
            .expect_err("unknown privacy should require explicit confirmation");
    assert!(error.to_string().contains("privacy is unknown"));

    let attached = attach_calendar_event_context_command(&store, "meeting-1", unknown, true, 2_100)
        .expect("confirmed unknown privacy event");
    assert!(attached.privacy_confirmed);
}

#[test]
fn attach_calendar_event_context_rejects_unsafe_event_shapes() {
    let cases = [
        ("not attachable", {
            let mut event = event("event-1", "Design Review");
            event.attachable = false;
            event
        }),
        ("empty id", {
            let mut event = event("", "Design Review");
            event.id = String::new();
            event
        }),
        ("invalid timing", {
            let mut event = event("event-1", "Design Review");
            event.ends_at_ms = event.starts_at_ms;
            event
        }),
        ("all day", {
            let mut event = event("event-1", "Design Review");
            event.is_all_day = true;
            event
        }),
        ("recurring", {
            let mut event = event("event-1", "Design Review");
            event.is_recurring = true;
            event
        }),
        ("overlapping", {
            let mut event = event("event-1", "Design Review");
            event.overlap_state = "Overlapping".to_string();
            event
        }),
        ("ambiguous", {
            let mut event = event("event-1", "Design Review");
            event.overlap_state = "Ambiguous".to_string();
            event
        }),
        ("private", {
            let mut event = event("event-1", "Design Review");
            event.privacy = "Private".to_string();
            event
        }),
    ];

    for (name, event) in cases {
        let store = migrated_store_with_meeting(name);
        attach_calendar_event_context_command(&store, "meeting-1", event, true, 2_000)
            .expect_err("unsafe event should reject");
        assert_eq!(
            store
                .meeting_calendar_context("meeting-1")
                .expect("calendar context"),
            None,
            "{name} should not persist an attachment"
        );
    }
}

fn event(id: &str, title: &str) -> CalendarEventAttachmentDto {
    CalendarEventAttachmentDto {
        id: id.to_string(),
        title: title.to_string(),
        calendar_title: "Work".to_string(),
        starts_at_ms: 1_000,
        ends_at_ms: 2_000,
        is_all_day: false,
        is_recurring: false,
        privacy: "Public".to_string(),
        overlap_state: "None".to_string(),
        attachable: true,
    }
}
