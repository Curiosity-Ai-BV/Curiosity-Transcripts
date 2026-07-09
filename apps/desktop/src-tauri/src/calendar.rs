use serde::Serialize;

#[cfg(all(target_os = "macos", not(test)))]
use objc2::rc::Retained;
#[cfg(all(target_os = "macos", not(test)))]
use objc2::runtime::Bool;
#[cfg(all(target_os = "macos", not(test)))]
use objc2::{available, msg_send};
#[cfg(all(target_os = "macos", not(test)))]
use objc2_event_kit::{
    EKAuthorizationStatus, EKCalendar, EKEntityType, EKEvent, EKEventStatus, EKEventStore,
};
#[cfg(all(target_os = "macos", not(test)))]
use objc2_foundation::{NSDate, NSError, NSString};

const APPLE_CALENDAR_LOOKAHEAD_HOURS: u64 = 24;
const APPLE_CALENDAR_MAX_EVENTS: usize = 20;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CalendarContextView {
    pub(crate) source: String,
    pub(crate) permission_state: String,
    pub(crate) availability_state: String,
    pub(crate) message: String,
    pub(crate) setup_guidance: String,
    pub(crate) upcoming_events: Vec<CalendarContextEventView>,
    pub(crate) auto_start_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CalendarContextEventView {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) calendar_title: String,
    pub(crate) starts_at_ms: u64,
    pub(crate) ends_at_ms: u64,
    pub(crate) is_all_day: bool,
    pub(crate) is_recurring: bool,
    pub(crate) privacy: String,
    pub(crate) overlap_state: String,
    pub(crate) attachable: bool,
    pub(crate) safety_note: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppleCalendarAuthorizationStatus {
    NotDetermined,
    FullAccess,
    WriteOnly,
    Denied,
    Restricted,
    Unavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppleCalendarAccessRequestApi {
    FullAccess,
    LegacyEventAccess,
}

#[derive(Clone, Debug)]
pub(crate) struct CalendarContextEventDraft {
    pub(crate) event: CalendarContextEventView,
    pub(crate) has_stable_identifier: bool,
}

pub(crate) fn calendar_context_snapshot(
    authorization_status: Option<AppleCalendarAuthorizationStatus>,
) -> CalendarContextView {
    let status = authorization_status.unwrap_or_else(apple_calendar_authorization_status);
    calendar_context_from_authorization(status)
}

#[cfg(test)]
pub(crate) fn request_apple_calendar_access_context() -> CalendarContextView {
    calendar_context_from_authorization(request_apple_calendar_full_access())
}

fn apple_calendar_access_request_api_for_availability(
    full_access_api_available: bool,
) -> AppleCalendarAccessRequestApi {
    if full_access_api_available {
        AppleCalendarAccessRequestApi::FullAccess
    } else {
        AppleCalendarAccessRequestApi::LegacyEventAccess
    }
}

#[cfg(all(target_os = "macos", not(test)))]
fn apple_calendar_access_request_api() -> AppleCalendarAccessRequestApi {
    apple_calendar_access_request_api_for_availability(available!(macos = 14.0))
}

fn calendar_context_from_authorization(
    status: AppleCalendarAuthorizationStatus,
) -> CalendarContextView {
    let upcoming_events = if status == AppleCalendarAuthorizationStatus::FullAccess {
        load_upcoming_apple_calendar_events()
    } else {
        Vec::new()
    };
    let granted_message = if upcoming_events.is_empty() {
        format!(
            "Apple Calendar access is granted; no upcoming events found in the next {APPLE_CALENDAR_LOOKAHEAD_HOURS} hours."
        )
    } else {
        format!(
            "Apple Calendar access is granted; {} upcoming events loaded for manual review.",
            upcoming_events.len()
        )
    };
    let (permission_state, availability_state, message, setup_guidance) = match status {
        AppleCalendarAuthorizationStatus::NotDetermined => (
            "NotRequested",
            "PermissionRequired",
            "Apple Calendar permission has not been requested.",
            "Use Request calendar access when you want Curiosity to read upcoming local Calendar events. Calendar events never start recordings automatically.",
        ),
        AppleCalendarAuthorizationStatus::FullAccess => (
            "Granted",
            "Ready",
            granted_message.as_str(),
            "Upcoming local events stay read-only until you explicitly attach one as meeting context. Calendar events never start recordings automatically.",
        ),
        AppleCalendarAuthorizationStatus::WriteOnly => (
            "Unavailable",
            "Unavailable",
            "Apple Calendar write-only access is not enough for meeting context.",
            "Grant full Calendar access before loading upcoming local events. Calendar events never start recordings automatically.",
        ),
        AppleCalendarAuthorizationStatus::Denied => (
            "Denied",
            "Unavailable",
            "Apple Calendar access is denied.",
            "Open macOS Privacy & Security > Calendars to grant access. Calendar events never start recordings automatically.",
        ),
        AppleCalendarAuthorizationStatus::Restricted => (
            "Unavailable",
            "Unavailable",
            "Apple Calendar access is restricted by macOS.",
            "Check macOS Calendar privacy restrictions before using Calendar context. Calendar events never start recordings automatically.",
        ),
        AppleCalendarAuthorizationStatus::Unavailable => (
            "Unavailable",
            "Unavailable",
            "Apple Calendar context requires macOS EventKit.",
            "Calendar context is read-only here, and recordings never start from calendar events automatically.",
        ),
        AppleCalendarAuthorizationStatus::Unknown => (
            "Unavailable",
            "Unavailable",
            "Apple Calendar authorization returned an unknown status.",
            "Check macOS Calendar privacy settings before using Calendar context. Calendar events never start recordings automatically.",
        ),
    };
    CalendarContextView {
        source: "AppleCalendar".to_string(),
        permission_state: permission_state.to_string(),
        availability_state: availability_state.to_string(),
        message: message.to_string(),
        setup_guidance: setup_guidance.to_string(),
        upcoming_events,
        auto_start_enabled: false,
    }
}

pub(crate) fn finalize_calendar_context_events(
    mut drafts: Vec<CalendarContextEventDraft>,
) -> Vec<CalendarContextEventView> {
    drafts.sort_by(|left, right| {
        left.event
            .starts_at_ms
            .cmp(&right.event.starts_at_ms)
            .then_with(|| left.event.ends_at_ms.cmp(&right.event.ends_at_ms))
            .then_with(|| left.event.title.cmp(&right.event.title))
            .then_with(|| left.event.id.cmp(&right.event.id))
    });

    let overlap_states: Vec<&'static str> = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            if !draft.has_stable_identifier || draft.event.starts_at_ms >= draft.event.ends_at_ms {
                return "Ambiguous";
            }
            let duplicate_identifier = drafts.iter().enumerate().any(|(other_index, other)| {
                index != other_index && draft.event.id == other.event.id
            });
            if duplicate_identifier {
                return "Ambiguous";
            }
            let overlaps = drafts.iter().enumerate().any(|(other_index, other)| {
                index != other_index
                    && calendar_event_intervals_overlap(
                        draft.event.starts_at_ms,
                        draft.event.ends_at_ms,
                        other.event.starts_at_ms,
                        other.event.ends_at_ms,
                    )
            });
            if overlaps {
                "Overlapping"
            } else {
                "None"
            }
        })
        .collect();

    for (draft, overlap_state) in drafts.iter_mut().zip(overlap_states) {
        draft.event.overlap_state = overlap_state.to_string();
        draft.event.attachable = calendar_event_can_attach(draft);
        draft.event.safety_note = calendar_event_safety_note(draft);
    }

    drafts
        .into_iter()
        .take(APPLE_CALENDAR_MAX_EVENTS)
        .map(|draft| draft.event)
        .collect()
}

fn calendar_event_intervals_overlap(
    starts_at_ms: u64,
    ends_at_ms: u64,
    other_starts_at_ms: u64,
    other_ends_at_ms: u64,
) -> bool {
    starts_at_ms < other_ends_at_ms && other_starts_at_ms < ends_at_ms
}

fn calendar_event_safety_note(draft: &CalendarContextEventDraft) -> String {
    let event = &draft.event;
    if !draft.has_stable_identifier {
        return "Event identifier is unstable; attachment is disabled.".to_string();
    }
    if event.starts_at_ms >= event.ends_at_ms {
        return "Event timing is ambiguous; attachment is disabled.".to_string();
    }
    if event.is_all_day {
        return "All-day event; attachment is disabled until all-day handling is implemented."
            .to_string();
    }
    if event.is_recurring {
        return "Recurring event; attachment is disabled until recurrence handling is implemented."
            .to_string();
    }
    if event.overlap_state == "Overlapping" {
        return "Overlaps another event; attachment is disabled until ambiguity handling is implemented."
            .to_string();
    }
    if event.overlap_state == "Ambiguous" {
        return "Ambiguous event; attachment is disabled.".to_string();
    }
    if event.privacy == "Private" {
        return "Private event; attachment is disabled.".to_string();
    }
    if event.privacy == "Unknown" {
        return "Privacy classification is unavailable from EventKit; confirm this event title is safe before attaching."
            .to_string();
    }
    "Ready for manual attachment. Calendar events never start recordings automatically.".to_string()
}

fn calendar_event_can_attach(draft: &CalendarContextEventDraft) -> bool {
    let event = &draft.event;
    draft.has_stable_identifier
        && event.starts_at_ms < event.ends_at_ms
        && !event.is_all_day
        && !event.is_recurring
        && event.overlap_state == "None"
        && matches!(event.privacy.as_str(), "Public" | "Unknown")
}

#[cfg(test)]
fn load_upcoming_apple_calendar_events() -> Vec<CalendarContextEventView> {
    Vec::new()
}

#[cfg(all(target_os = "macos", not(test)))]
fn load_upcoming_apple_calendar_events() -> Vec<CalendarContextEventView> {
    let now_ms = current_unix_time_ms();
    let start_seconds = now_ms as f64 / 1_000.0;
    let end_seconds =
        start_seconds + (APPLE_CALENDAR_LOOKAHEAD_HOURS.saturating_mul(60 * 60)) as f64;
    let start_date = NSDate::dateWithTimeIntervalSince1970(start_seconds);
    let end_date = NSDate::dateWithTimeIntervalSince1970(end_seconds);
    let store = unsafe { EKEventStore::new() };
    let predicate = unsafe {
        store.predicateForEventsWithStartDate_endDate_calendars(&start_date, &end_date, None)
    };
    let events = unsafe { store.eventsMatchingPredicate(&predicate) };
    let drafts = events
        .iter()
        .filter_map(|event| unsafe { calendar_context_event_draft_from_event(&event) })
        .collect();

    finalize_calendar_context_events(drafts)
}

#[cfg(not(any(target_os = "macos", test)))]
fn load_upcoming_apple_calendar_events() -> Vec<CalendarContextEventView> {
    Vec::new()
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe fn calendar_context_event_draft_from_event(
    event: &EKEvent,
) -> Option<CalendarContextEventDraft> {
    if unsafe { event.status() } == EKEventStatus::Canceled {
        return None;
    }
    let event_identifier = unsafe { event.eventIdentifier() };
    let start_date = unsafe { event.startDate() };
    let end_date = unsafe { event.endDate() };
    let starts_at_ms = calendar_date_ms(&start_date);
    let ends_at_ms = calendar_date_ms(&end_date);
    if starts_at_ms == 0 || ends_at_ms <= starts_at_ms {
        return None;
    }
    let title = calendar_text_or_fallback(
        &unsafe { calendar_event_title(event) },
        "Untitled calendar event",
    );
    let calendar_title = calendar_text_or_fallback(
        &unsafe { calendar_event_calendar_title(event) },
        "Unknown calendar",
    );
    let stable_id = event_identifier
        .as_ref()
        .map(|identifier| identifier.to_string())
        .filter(|identifier| !identifier.trim().is_empty());
    let fallback_stable_id = if stable_id.is_none() {
        unsafe { calendar_event_calendar_item_identifier(event) }
            .into_iter()
            .find(|identifier| !identifier.trim().is_empty())
            .map(|identifier| format!("{identifier}-{starts_at_ms}"))
    } else {
        None
    };
    let has_stable_identifier = stable_id.is_some() || fallback_stable_id.is_some();
    let id = stable_id
        .or(fallback_stable_id)
        .unwrap_or_else(|| format!("calendar-event-{starts_at_ms}-{ends_at_ms}"));
    let is_recurring = unsafe {
        calendar_event_has_recurrence_rules(event)
            || event.occurrenceDate().is_some()
            || event.isDetached()
    };

    Some(CalendarContextEventDraft {
        event: CalendarContextEventView {
            id,
            title,
            calendar_title,
            starts_at_ms,
            ends_at_ms,
            is_all_day: unsafe { event.isAllDay() },
            is_recurring,
            privacy: "Unknown".to_string(),
            overlap_state: "None".to_string(),
            attachable: false,
            safety_note: String::new(),
        },
        has_stable_identifier,
    })
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe fn calendar_event_title(event: &EKEvent) -> String {
    let title: Retained<NSString> = unsafe { msg_send![event, title] };
    title.to_string()
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe fn calendar_event_calendar_title(event: &EKEvent) -> String {
    let calendar: Option<Retained<EKCalendar>> = unsafe { msg_send![event, calendar] };
    calendar
        .map(|calendar| unsafe { calendar.title() }.to_string())
        .unwrap_or_default()
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe fn calendar_event_calendar_item_identifier(event: &EKEvent) -> Option<String> {
    let identifier: Option<Retained<NSString>> =
        unsafe { msg_send![event, calendarItemIdentifier] };
    identifier.map(|identifier| identifier.to_string())
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe fn calendar_event_has_recurrence_rules(event: &EKEvent) -> bool {
    unsafe { msg_send![event, hasRecurrenceRules] }
}

#[cfg(all(target_os = "macos", not(test)))]
fn calendar_date_ms(date: &NSDate) -> u64 {
    let seconds = date.timeIntervalSince1970();
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds * 1_000.0).round() as u64
}

#[cfg(all(target_os = "macos", not(test)))]
fn calendar_text_or_fallback(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(all(target_os = "macos", not(test)))]
fn current_unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
fn apple_calendar_authorization_status() -> AppleCalendarAuthorizationStatus {
    AppleCalendarAuthorizationStatus::NotDetermined
}

#[cfg(all(target_os = "macos", not(test)))]
fn apple_calendar_authorization_status() -> AppleCalendarAuthorizationStatus {
    map_eventkit_authorization_status(unsafe {
        EKEventStore::authorizationStatusForEntityType(EKEntityType::Event)
    })
}

#[cfg(not(any(target_os = "macos", test)))]
fn apple_calendar_authorization_status() -> AppleCalendarAuthorizationStatus {
    AppleCalendarAuthorizationStatus::Unavailable
}

#[cfg(test)]
pub(crate) fn request_apple_calendar_full_access() -> AppleCalendarAuthorizationStatus {
    AppleCalendarAuthorizationStatus::FullAccess
}

#[cfg(all(target_os = "macos", not(test)))]
pub(crate) fn request_apple_calendar_full_access() -> AppleCalendarAuthorizationStatus {
    use std::sync::mpsc;
    use std::time::Duration;

    let current_status = apple_calendar_authorization_status();
    if current_status != AppleCalendarAuthorizationStatus::NotDetermined {
        return current_status;
    }
    let store = unsafe { EKEventStore::new() };
    let (sender, receiver) = mpsc::channel::<bool>();
    let block = block2::RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let _ = sender.send(granted.as_bool());
    });

    unsafe {
        match apple_calendar_access_request_api() {
            AppleCalendarAccessRequestApi::FullAccess => {
                store.requestFullAccessToEventsWithCompletion(block2::RcBlock::as_ptr(&block));
            }
            AppleCalendarAccessRequestApi::LegacyEventAccess => {
                #[allow(deprecated)]
                store.requestAccessToEntityType_completion(
                    EKEntityType::Event,
                    block2::RcBlock::as_ptr(&block),
                );
            }
        }
    }

    match receiver.recv_timeout(Duration::from_secs(300)) {
        Ok(true) => AppleCalendarAuthorizationStatus::FullAccess,
        Ok(false) => apple_calendar_authorization_status(),
        Err(_) => {
            // EventKit may still hold and call the completion block after our
            // timeout. Keep the callback state and store alive rather than
            // freeing closure memory under a late OS callback.
            std::mem::forget(block);
            std::mem::forget(store);
            AppleCalendarAuthorizationStatus::Unavailable
        }
    }
}

#[cfg(not(any(target_os = "macos", test)))]
pub(crate) fn request_apple_calendar_full_access() -> AppleCalendarAuthorizationStatus {
    AppleCalendarAuthorizationStatus::Unavailable
}

#[cfg(all(target_os = "macos", not(test)))]
fn map_eventkit_authorization_status(
    status: EKAuthorizationStatus,
) -> AppleCalendarAuthorizationStatus {
    if status == EKAuthorizationStatus::NotDetermined {
        AppleCalendarAuthorizationStatus::NotDetermined
    } else if status == EKAuthorizationStatus::Restricted {
        AppleCalendarAuthorizationStatus::Restricted
    } else if status == EKAuthorizationStatus::Denied {
        AppleCalendarAuthorizationStatus::Denied
    } else if status == EKAuthorizationStatus::FullAccess {
        AppleCalendarAuthorizationStatus::FullAccess
    } else if status == EKAuthorizationStatus::WriteOnly {
        AppleCalendarAuthorizationStatus::WriteOnly
    } else {
        AppleCalendarAuthorizationStatus::Unknown
    }
}

#[cfg(test)]
pub(crate) fn calendar_event_draft(
    id: &str,
    title: &str,
    starts_at_ms: u64,
    ends_at_ms: u64,
) -> CalendarContextEventDraft {
    CalendarContextEventDraft {
        event: CalendarContextEventView {
            id: id.to_string(),
            title: title.to_string(),
            calendar_title: "Work".to_string(),
            starts_at_ms,
            ends_at_ms,
            is_all_day: false,
            is_recurring: false,
            privacy: "Public".to_string(),
            overlap_state: "None".to_string(),
            attachable: false,
            safety_note: String::new(),
        },
        has_stable_identifier: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_calendar_permission_request_returns_granted_context_without_events_or_autostart() {
        let context = request_apple_calendar_access_context();
        let json = serde_json::to_value(&context).expect("serialize calendar context");

        assert_eq!(json["source"], "AppleCalendar");
        assert_eq!(json["permissionState"], "Granted");
        assert_eq!(json["availabilityState"], "Ready");
        assert_eq!(json["autoStartEnabled"], false);
        assert_eq!(
            json["upcomingEvents"]
                .as_array()
                .expect("upcoming calendar events")
                .len(),
            0
        );
        assert!(json["setupGuidance"]
            .as_str()
            .expect("setup guidance")
            .contains("never start recordings automatically"));
    }

    #[test]
    fn calendar_authorization_statuses_map_to_safe_snapshot_states() {
        let cases = [
            (
                AppleCalendarAuthorizationStatus::WriteOnly,
                "Unavailable",
                "Unavailable",
            ),
            (
                AppleCalendarAuthorizationStatus::Denied,
                "Denied",
                "Unavailable",
            ),
            (
                AppleCalendarAuthorizationStatus::Restricted,
                "Unavailable",
                "Unavailable",
            ),
            (
                AppleCalendarAuthorizationStatus::Unavailable,
                "Unavailable",
                "Unavailable",
            ),
            (
                AppleCalendarAuthorizationStatus::Unknown,
                "Unavailable",
                "Unavailable",
            ),
        ];

        for (status, expected_permission, expected_availability) in cases {
            let context = calendar_context_from_authorization(status);
            assert_eq!(context.permission_state, expected_permission);
            assert_eq!(context.availability_state, expected_availability);
            assert!(!context.auto_start_enabled);
            assert!(context.upcoming_events.is_empty());
        }
    }

    #[test]
    fn calendar_event_finalization_marks_overlaps_and_blocks_unsafe_shapes() {
        let design_review = calendar_event_draft("event-1", "Design Review", 1_000, 2_000);
        let mut planning = calendar_event_draft("event-2", "Planning", 1_500, 2_500);
        planning.event.is_recurring = true;
        let mut offsite = calendar_event_draft("event-3", "Offsite", 3_000, 4_000);
        offsite.event.is_all_day = true;

        let events = finalize_calendar_context_events(vec![design_review, planning, offsite]);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, "event-1");
        assert_eq!(events[0].overlap_state, "Overlapping");
        assert!(!events[0].attachable);
        assert!(events[0].safety_note.contains("Overlaps another event"));
        assert_eq!(events[1].overlap_state, "Overlapping");
        assert!(!events[1].attachable);
        assert!(events[1].safety_note.contains("Recurring event"));
        assert_eq!(events[2].overlap_state, "None");
        assert!(!events[2].attachable);
        assert!(events[2].safety_note.contains("All-day event"));
    }

    #[test]
    fn calendar_event_finalization_blocks_unstable_and_requires_unknown_privacy_confirmation() {
        let mut missing_id = calendar_event_draft("synthetic", "Missing ID", 1_000, 2_000);
        missing_id.has_stable_identifier = false;
        let mut unknown_privacy = calendar_event_draft("event-2", "Normal Event", 3_000, 4_000);
        unknown_privacy.event.privacy = "Unknown".to_string();
        let duplicate_first = calendar_event_draft("duplicate", "Duplicate One", 5_000, 6_000);
        let duplicate_second = calendar_event_draft("duplicate", "Duplicate Two", 7_000, 8_000);

        let events = finalize_calendar_context_events(vec![
            missing_id,
            unknown_privacy,
            duplicate_first,
            duplicate_second,
        ]);

        assert_eq!(events[0].overlap_state, "Ambiguous");
        assert!(!events[0].attachable);
        assert!(events[0].safety_note.contains("identifier is unstable"));
        assert_eq!(events[1].overlap_state, "None");
        assert!(events[1].attachable);
        assert!(events[1]
            .safety_note
            .contains("confirm this event title is safe"));
        assert_eq!(events[2].overlap_state, "Ambiguous");
        assert_eq!(events[3].overlap_state, "Ambiguous");
    }

    #[test]
    fn calendar_access_request_api_preserves_macos_13_support_floor() {
        assert_eq!(
            apple_calendar_access_request_api_for_availability(false),
            AppleCalendarAccessRequestApi::LegacyEventAccess
        );
        assert_eq!(
            apple_calendar_access_request_api_for_availability(true),
            AppleCalendarAccessRequestApi::FullAccess
        );
    }
}
