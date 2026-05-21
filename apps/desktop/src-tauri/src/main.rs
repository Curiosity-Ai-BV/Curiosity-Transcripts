use std::path::{Path, PathBuf};

use curiosity_app::{
    list_meetings_dto, meeting_detail_dto, AppPermissionState, CommandRecordingDto,
    CommandRecordingState, RawAudioRetentionPolicy, StorageLocationDto,
};
use curiosity_audio::{
    ManualSmokeCheck, ManualSmokeResult, ManualSmokeStatus, ScreenCaptureKitSystemAudioAdapter,
    SystemAudioAdapterStatus,
};
use curiosity_store::Store;
use serde::Serialize;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            desktop_snapshot,
            audio_smoke_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Curiosity Transcripts desktop shell");
}

#[tauri::command]
fn desktop_snapshot(app: tauri::AppHandle) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    desktop_snapshot_for_app_root(&app_root)
}

#[tauri::command]
fn audio_smoke_status() -> AudioSmokeStatus {
    build_audio_smoke_status()
}

fn desktop_snapshot_for_app_root(app_root: &Path) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    let meeting_summaries = list_meetings_dto(&store).map_err(|error| error.to_string())?;
    let mut meetings = Vec::with_capacity(meeting_summaries.len());

    for summary in meeting_summaries {
        let detail =
            meeting_detail_dto(&store, &summary.meeting_id).map_err(|error| error.to_string())?;
        let analysis = store
            .current_analysis_result(&summary.meeting_id)
            .map_err(|error| error.to_string())?;
        let transcript_text = detail
            .transcript_segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let segments = detail
            .transcript_segments
            .into_iter()
            .map(|segment| TranscriptSegmentView {
                id: segment.id,
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                text: segment.text,
                source_channel: segment.source_channel,
                model_run_id: segment.model_run_id,
                transcript_version_id: segment.transcript_version_id,
            })
            .collect();

        meetings.push(MeetingView {
            id: summary.meeting_id.clone(),
            title: detail.title,
            started_at: format_timestamp(summary.started_at_ms),
            duration: format_duration(summary.started_at_ms, summary.ended_at_ms),
            status: summary.status,
            transcript_state: map_transcript_state(&summary.transcript_state),
            transcript_text,
            segments,
            privacy: MeetingPrivacy {
                storage_label: "Private storage".to_string(),
                storage_path: format!("meetings/{}/audio", summary.meeting_id),
                raw_audio_retention: RawAudioRetentionPolicy::Retain,
                local_only: analysis
                    .as_ref()
                    .map(|analysis| !analysis.network_used)
                    .unwrap_or(true),
            },
            export_state: ExportCommandState::default(),
            delete_state: DeleteCommandState::default(),
            analysis: analysis.map(|analysis| AnalysisDisclosureState {
                provider: analysis.provider,
                model_name: analysis.model_name,
                network_used: analysis.network_used,
                disclosure_required: analysis.network_used,
                disclosure_confirmed: false,
            }),
        });
    }

    let selected_meeting_id = meetings.first().map(|meeting| meeting.id.clone());

    Ok(DesktopSnapshot {
        loading: false,
        command_surface: CommandSurfaceState {
            detail: "Connected to local desktop commands.".to_string(),
        },
        meetings,
        selected_meeting_id,
        recording: CommandRecordingDto {
            meeting_id: String::new(),
            recording_id: None,
            state: CommandRecordingState::Interrupted,
            permission_state: AppPermissionState::MicrophoneUnavailable,
            storage_location: StorageLocationDto {
                app_private_path: app_root.display().to_string(),
            },
            raw_audio_retention: RawAudioRetentionPolicy::Retain,
            recoverable: false,
            recovery_action: "Recording commands are not wired into the desktop shell yet."
                .to_string(),
        },
        model: model_status_from_env(),
        capture: CaptureStatus {
            microphone: DesktopPermissionState::MicrophoneUnavailable,
            system_audio: DesktopPermissionState::SystemAudioUnavailable,
        },
    })
}

fn open_store(app_root: &Path) -> Result<Store, String> {
    std::fs::create_dir_all(app_root).map_err(|error| error.to_string())?;
    let store = Store::open(app_root.join("curiosity.sqlite3"), app_root.to_path_buf())
        .map_err(|error| error.to_string())?;
    store.migrate().map_err(|error| error.to_string())?;
    Ok(store)
}

fn model_status_from_env() -> ModelStatus {
    let configured_path = std::env::var("CURIOSITY_WHISPER_MODEL").unwrap_or_default();
    let kind = if !configured_path.is_empty() && PathBuf::from(&configured_path).is_file() {
        "ready"
    } else {
        "missing"
    };
    ModelStatus {
        kind: kind.to_string(),
        configured_path,
    }
}

fn build_audio_smoke_status() -> AudioSmokeStatus {
    let microphone =
        smoke_result_status(ManualSmokeCheck::macos_placeholder().run_without_hardware());
    let system_audio = match ScreenCaptureKitSystemAudioAdapter::status() {
        SystemAudioAdapterStatus::Available => CaptureProbeStatus {
            state: "Available".to_string(),
            message: "System audio adapter is available.".to_string(),
        },
        SystemAudioAdapterStatus::PermissionDenied(error) => CaptureProbeStatus {
            state: "PermissionDenied".to_string(),
            message: error.recovery_guidance().steps.join("; "),
        },
        SystemAudioAdapterStatus::Unavailable(error) => CaptureProbeStatus {
            state: "Unavailable".to_string(),
            message: error.recovery_guidance().steps.join("; "),
        },
    };

    AudioSmokeStatus {
        microphone,
        system_audio,
    }
}

fn smoke_result_status(result: ManualSmokeResult) -> CaptureProbeStatus {
    let state = match result.status {
        ManualSmokeStatus::NotRun => "NotRun",
        ManualSmokeStatus::Skipped => "Skipped",
        ManualSmokeStatus::Unavailable => "Unavailable",
        ManualSmokeStatus::PermissionDenied => "PermissionDenied",
        ManualSmokeStatus::Passed => "Passed",
    };
    CaptureProbeStatus {
        state: state.to_string(),
        message: result.message,
    }
}

fn map_transcript_state(state: &str) -> TranscriptStateView {
    match state {
        "Complete" => TranscriptStateView::Ready,
        "Transcribing" => TranscriptStateView::Transcribing,
        _ => TranscriptStateView::Unavailable,
    }
}

fn format_timestamp(timestamp_ms: u64) -> String {
    if timestamp_ms == 0 {
        return "0 ms".to_string();
    }
    format!("{timestamp_ms} ms")
}

fn format_duration(started_at_ms: u64, ended_at_ms: Option<u64>) -> String {
    let Some(ended_at_ms) = ended_at_ms else {
        return "In progress".to_string();
    };
    let minutes = ended_at_ms.saturating_sub(started_at_ms) / 60_000;
    format!("{minutes}m")
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSnapshot {
    loading: bool,
    command_surface: CommandSurfaceState,
    meetings: Vec<MeetingView>,
    selected_meeting_id: Option<String>,
    recording: CommandRecordingDto,
    model: ModelStatus,
    capture: CaptureStatus,
}

#[derive(Clone, Debug, Serialize)]
struct CommandSurfaceState {
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelStatus {
    kind: String,
    configured_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureStatus {
    microphone: DesktopPermissionState,
    system_audio: DesktopPermissionState,
}

#[derive(Clone, Copy, Debug, Serialize)]
enum DesktopPermissionState {
    MicrophoneUnavailable,
    SystemAudioUnavailable,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingView {
    id: String,
    title: String,
    started_at: String,
    duration: String,
    status: String,
    transcript_state: TranscriptStateView,
    transcript_text: String,
    segments: Vec<TranscriptSegmentView>,
    privacy: MeetingPrivacy,
    export_state: ExportCommandState,
    delete_state: DeleteCommandState,
    analysis: Option<AnalysisDisclosureState>,
}

#[derive(Clone, Copy, Debug, Serialize)]
enum TranscriptStateView {
    Ready,
    Transcribing,
    Unavailable,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptSegmentView {
    id: String,
    start_ms: u64,
    end_ms: u64,
    text: String,
    source_channel: String,
    model_run_id: String,
    transcript_version_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingPrivacy {
    storage_label: String,
    storage_path: String,
    raw_audio_retention: RawAudioRetentionPolicy,
    local_only: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ExportCommandState {
    state: &'static str,
}

impl Default for ExportCommandState {
    fn default() -> Self {
        Self { state: "idle" }
    }
}

#[derive(Clone, Debug, Serialize)]
struct DeleteCommandState {
    state: &'static str,
}

impl Default for DeleteCommandState {
    fn default() -> Self {
        Self { state: "idle" }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisDisclosureState {
    provider: String,
    model_name: String,
    network_used: bool,
    disclosure_required: bool,
    disclosure_confirmed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioSmokeStatus {
    microphone: CaptureProbeStatus,
    system_audio: CaptureProbeStatus,
}

#[derive(Clone, Debug, Serialize)]
struct CaptureProbeStatus {
    state: String,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use curiosity_domain::{
        AnalysisCitation, Meeting, MeetingAnalysis, ModelRun, SourceChannel, TranscriptSegment,
        TranscriptVersion,
    };
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn empty_desktop_snapshot_serializes_frontend_shape() {
        let root = unique_test_root();
        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["loading"], false);
        assert_eq!(
            json["commandSurface"]["detail"],
            "Connected to local desktop commands."
        );
        assert_eq!(json["meetings"].as_array().expect("meetings").len(), 0);
        assert!(json["selectedMeetingId"].is_null());
        assert_eq!(json["recording"]["permission_state"], "MicrophoneUnavailable");
        assert_eq!(
            json["recording"]["recovery_action"],
            "Recording commands are not wired into the desktop shell yet."
        );
        assert_eq!(
            json["recording"]["storage_location"]["app_private_path"],
            root.display().to_string()
        );
        assert_eq!(json["model"]["kind"], "missing");
        assert_eq!(json["capture"]["microphone"], "MicrophoneUnavailable");
        assert_eq!(json["capture"]["systemAudio"], "SystemAudioUnavailable");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn meeting_without_analysis_serializes_no_summary_instead_of_fake_local_result() {
        let root = unique_test_root();
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
            .expect("insert meeting");
        drop(store);

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert!(json["meetings"][0]["analysis"].is_null());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn non_empty_desktop_snapshot_serializes_frontend_dto_shape() {
        let root = unique_test_root();
        seed_transcribed_analyzed_meeting(&root);

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let meeting = &json["meetings"][0];
        let segment = &meeting["segments"][0];

        assert_eq!(meeting["startedAt"], "1000 ms");
        assert_eq!(meeting["transcriptState"], "Ready");
        assert_eq!(segment["startMs"], 0);
        assert_eq!(segment["modelRunId"], "run-1");
        assert_eq!(segment["sourceChannel"], "Microphone");
        assert_eq!(meeting["privacy"]["storagePath"], "meetings/meeting-1/audio");
        assert_eq!(meeting["analysis"]["modelName"], "qwen3:30b");
        assert_eq!(meeting["analysis"]["networkUsed"], false);

        fs::remove_dir_all(root).expect("cleanup");
    }

    fn seed_transcribed_analyzed_meeting(root: &Path) {
        let store = open_store(root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
            .expect("insert meeting");
        let run = ModelRun::new(
            "run-1",
            "meeting-1",
            "sha256:audio",
            "whisper-rs",
            "fixture-whisper",
            false,
            2_000,
        );
        let version = TranscriptVersion::new("version-1", "meeting-1", "run-1", 1, 2_010);
        let segments = vec![TranscriptSegment::with_metadata(
            "segment-1",
            "meeting-1",
            0,
            1_200,
            "We decided to keep transcripts local.",
            SourceChannel::Microphone,
            "run-1",
            "version-1",
        )];
        store
            .persist_transcript(&run, &version, &segments)
            .expect("persist transcript");
        store
            .persist_analysis_result(&MeetingAnalysis {
                id: "analysis-1".to_string(),
                meeting_id: "meeting-1".to_string(),
                provider: "ollama".to_string(),
                model_name: "qwen3:30b".to_string(),
                network_used: false,
                created_at_ms: 3_000,
                prompt_template_version: "summary-v1".to_string(),
                summary: "Local summary".to_string(),
                decisions: Vec::new(),
                action_items: Vec::new(),
                questions: Vec::new(),
                citations: vec![AnalysisCitation {
                    segment_id: "segment-1".to_string(),
                    start_ms: 0,
                    end_ms: 1_200,
                }],
            })
            .expect("persist analysis");
    }

    fn unique_test_root() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let suffix = TEST_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "curiosity-desktop-command-test-{nanos}-{suffix}"
        ))
    }
}
