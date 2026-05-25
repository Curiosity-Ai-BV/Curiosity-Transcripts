use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use curiosity_analysis::{
    recommended_analysis_model_presets, summary_json_schema, AnalysisClientError,
    AnalysisProviderKind, OllamaAnalyzer, ProviderTextClient,
};
use curiosity_app::{
    delete_meeting_command, export_meeting_json_command, generate_summary_command,
    list_meetings_dto, meeting_detail_dto, rename_meeting_command, search_meetings_dto,
    AnalysisCommandDto, AnalysisCommandState, AppPermissionState, CommandRecordingDto,
    CommandRecordingState, DeletedMeetingDto, ExportedMeetingDto, MeetingAnalysisDto,
    MeetingSearchResultDto, RawAudioRetentionPolicy, StorageLocationDto,
};
use curiosity_audio::{
    ArtifactManifest, CaptureCapability, CaptureError, CapturePermission, MacosDesktopWavRecording,
    MacosMicrophoneWavRecording, ManualSmokeCheck, ManualSmokeResult, ManualSmokeStatus,
    ScreenCaptureKitSystemAudioAdapter, StreamKind, SystemAudioAdapterStatus,
};
#[cfg(any(test, debug_assertions))]
use curiosity_domain::TranscriptSegment;
use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, MeetingStatus, ModelRun, RecordingSession,
    RecordingSource, RecordingStatus, SourceChannel, TranscriptVersion,
};
use curiosity_store::{AppSettings, CompletedAudioArtifact, RecoverableArtifact, Store};
#[cfg(feature = "whisper-rs")]
use curiosity_transcription::RealWhisperBackend;
use curiosity_transcription::{
    TranscriptionDocument, TranscriptionError, WhisperBackend, WhisperTranscriber,
    WhisperTranscriptionRequest,
};
use serde::Serialize;
use tauri::Manager;
use url::Url;

fn main() {
    let builder = tauri::Builder::default().manage(Mutex::new(DesktopCommandState::default()));
    #[cfg(any(test, debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        desktop_snapshot,
        search_meetings,
        rename_meeting,
        export_meeting_json,
        delete_meeting,
        generate_summary,
        get_settings,
        save_whisper_model_path,
        save_analysis_settings,
        test_whisper_model_path,
        test_ollama_connection,
        audio_smoke_status,
        system_audio_smoke_recording,
        start_microphone_recording,
        stop_microphone_recording,
        transcribe_meeting,
        seed_dev_fixture
    ]);
    #[cfg(not(any(test, debug_assertions)))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        desktop_snapshot,
        search_meetings,
        rename_meeting,
        export_meeting_json,
        delete_meeting,
        generate_summary,
        get_settings,
        save_whisper_model_path,
        save_analysis_settings,
        test_whisper_model_path,
        test_ollama_connection,
        audio_smoke_status,
        system_audio_smoke_recording,
        start_microphone_recording,
        stop_microphone_recording,
        transcribe_meeting
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("failed to run Curiosity Transcripts desktop shell");
}

#[tauri::command]
fn desktop_snapshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[cfg(any(test, debug_assertions))]
#[tauri::command]
fn seed_dev_fixture(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let command_state = state.lock().map_err(|error| error.to_string())?;
    seed_dev_fixture_for_app_root(&app_root, &command_state)
}

#[tauri::command]
fn search_meetings(
    app: tauri::AppHandle,
    query: String,
) -> Result<Vec<MeetingSearchResultDto>, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    search_meetings_for_app_root(&app_root, &query)
}

#[tauri::command]
fn rename_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
    title: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let mut command_state = state.lock().map_err(|error| error.to_string())?;
    rename_meeting_for_app_root(&app_root, &mut command_state, &meeting_id, &title)
}

#[tauri::command]
fn export_meeting_json(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let mut command_state = state.lock().map_err(|error| error.to_string())?;
    export_meeting_json_for_app_root(&app_root, &mut command_state, &meeting_id)
}

#[tauri::command]
fn delete_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let mut command_state = state.lock().map_err(|error| error.to_string())?;
    delete_meeting_for_app_root(&app_root, &mut command_state, &meeting_id)
}

#[tauri::command]
fn generate_summary(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let command = generate_summary_for_app_root(&app_root, &meeting_id)?;
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.last_analysis = Some(command);
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> Result<AppSettingsView, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    get_settings_for_app_root(&app_root)
}

#[tauri::command]
fn save_whisper_model_path(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    whisper_model_path: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    save_whisper_model_path_for_app_root(&app_root, whisper_model_path)?;
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn save_analysis_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    ollama_base_url: String,
    ollama_model: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    save_analysis_settings_for_app_root(&app_root, ollama_base_url, ollama_model)?;
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn test_whisper_model_path(path: String) -> WhisperModelPathTestView {
    test_whisper_model_path_value(&path)
}

#[tauri::command]
fn test_ollama_connection(base_url: String, model: String) -> OllamaConnectionTestView {
    test_ollama_connection_value(&base_url, &model, &UreqOllamaHttpTransport)
}

#[tauri::command]
fn audio_smoke_status() -> AudioSmokeStatus {
    build_audio_smoke_status()
}

#[tauri::command]
fn system_audio_smoke_recording(
    app: tauri::AppHandle,
    duration_ms: Option<u64>,
) -> Result<CaptureProbeStatus, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    Ok(system_audio_smoke_recording_for_app_root(
        &app_root,
        std::time::Duration::from_millis(duration_ms.unwrap_or(1_000).min(10_000)),
    ))
}

#[tauri::command]
fn start_microphone_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    title: Option<String>,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        if command_state.active_recording.is_some() || command_state.starting_recording {
            return Err("Stop the active recording before starting another one.".to_string());
        }
        command_state.starting_recording = true;
    }

    let mut started_state = DesktopCommandState::default();
    let started_at_ms = current_timestamp_ms();
    let result = start_microphone_recording_for_app_root(
        &app_root,
        &mut started_state,
        title,
        started_at_ms,
        &RealMicrophoneRecorderFactory,
    );
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.starting_recording = false;
        match result {
            Ok(_) => {
                command_state.active_recording = started_state.active_recording.take();
                command_state.last_recording = started_state.last_recording.take();
                command_state.last_transcription = None;
            }
            Err(error) => {
                if started_state.active_recording.is_some() {
                    // Snapshot assembly can fail after capture starts; keep the handle stoppable.
                    command_state.active_recording = started_state.active_recording.take();
                    command_state.last_recording = started_state.last_recording.take();
                    command_state.last_transcription = None;
                } else {
                    command_state.last_recording =
                        Some(start_failure_recording_dto(&app_root, &error));
                }
            }
        }
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn stop_microphone_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let active = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state
            .active_recording
            .take()
            .ok_or_else(|| "Start a desktop recording before stopping.".to_string())?
    };
    let recording = stop_active_microphone_recording(&app_root, active, current_timestamp_ms());
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.last_recording = Some(recording);
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn transcribe_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let settings = app_settings_for_app_root(&app_root)?;
    let model_path = resolved_whisper_model_path(&settings);
    let model_name = model_name_for_path(&model_path);

    #[cfg(feature = "whisper-rs")]
    {
        let transcription = transcribe_meeting_command(
            &app_root,
            &meeting_id,
            PathBuf::from(model_path),
            model_name,
            RealWhisperBackend,
            current_timestamp_ms(),
        )?;
        let snapshot_state = {
            let mut command_state = state.lock().map_err(|error| error.to_string())?;
            command_state.last_transcription = Some(transcription);
            command_state.snapshot_state()
        };
        desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
    }

    #[cfg(not(feature = "whisper-rs"))]
    {
        let transcription = transcribe_meeting_command(
            &app_root,
            &meeting_id,
            PathBuf::from(model_path),
            model_name,
            BackendUnavailableWhisperBackend,
            current_timestamp_ms(),
        )?;
        let snapshot_state = {
            let mut command_state = state.lock().map_err(|error| error.to_string())?;
            command_state.last_transcription = Some(transcription);
            command_state.snapshot_state()
        };
        desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
    }
}

#[cfg(test)]
fn desktop_snapshot_for_app_root(app_root: &Path) -> Result<DesktopSnapshot, String> {
    desktop_snapshot_for_app_root_with_state(app_root, &DesktopCommandSnapshotState::default())
}

fn desktop_snapshot_for_app_root_with_state(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    let settings = store.app_settings().map_err(|error| error.to_string())?;
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
            export_state: command_state
                .last_export
                .as_ref()
                .filter(|state| state.meeting_id.as_deref() == Some(summary.meeting_id.as_str()))
                .cloned()
                .unwrap_or_default(),
            delete_state: command_state
                .last_delete
                .as_ref()
                .filter(|state| state.meeting_id.as_deref() == Some(summary.meeting_id.as_str()))
                .cloned()
                .unwrap_or_default(),
            analysis: analysis.map(|analysis| AnalysisDisclosureState {
                provider: analysis.provider,
                model_name: analysis.model_name,
                network_used: analysis.network_used,
                disclosure_required: analysis.network_used,
                disclosure_confirmed: false,
                summary: analysis.summary,
                created_at_ms: analysis.created_at_ms,
                prompt_template_version: analysis.prompt_template_version,
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
        recording: recording_snapshot(app_root, command_state),
        model: model_status_from_settings(&settings),
        settings: app_settings_view(settings),
        capture: CaptureStatus {
            microphone: microphone_capture_state(command_state),
            system_audio: system_audio_capture_state(command_state),
        },
        transcription: command_state.last_transcription.clone(),
        export_command: command_state.last_export.clone().unwrap_or_default(),
        delete_command: command_state.last_delete.clone().unwrap_or_default(),
        analysis_command: command_state.last_analysis.clone(),
    })
}

fn open_store(app_root: &Path) -> Result<Store, String> {
    std::fs::create_dir_all(app_root).map_err(|error| error.to_string())?;
    let store = Store::open(app_root.join("curiosity.sqlite3"), app_root.to_path_buf())
        .map_err(|error| error.to_string())?;
    store.migrate().map_err(|error| error.to_string())?;
    store.repair_startup().map_err(|error| error.to_string())?;
    Ok(store)
}

fn app_settings_for_app_root(app_root: &Path) -> Result<AppSettings, String> {
    open_store(app_root)?
        .app_settings()
        .map_err(|error| error.to_string())
}

fn get_settings_for_app_root(app_root: &Path) -> Result<AppSettingsView, String> {
    app_settings_for_app_root(app_root).map(app_settings_view)
}

fn save_whisper_model_path_for_app_root(
    app_root: &Path,
    whisper_model_path: String,
) -> Result<AppSettingsView, String> {
    let store = open_store(app_root)?;
    store
        .save_whisper_model_path(&whisper_model_path)
        .map(app_settings_view)
        .map_err(|error| error.to_string())
}

fn save_analysis_settings_for_app_root(
    app_root: &Path,
    ollama_base_url: String,
    ollama_model: String,
) -> Result<AppSettingsView, String> {
    let store = open_store(app_root)?;
    store
        .save_analysis_settings(&ollama_base_url, &ollama_model)
        .map(app_settings_view)
        .map_err(|error| error.to_string())
}

fn search_meetings_for_app_root(
    app_root: &Path,
    query: &str,
) -> Result<Vec<MeetingSearchResultDto>, String> {
    let store = open_store(app_root)?;
    search_meetings_dto(&store, query).map_err(|error| error.to_string())
}

fn rename_meeting_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
    title: &str,
) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    rename_meeting_command(&store, meeting_id, title).map_err(|error| error.to_string())?;
    drop(store);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn export_meeting_json_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    let settings = store.app_settings().map_err(|error| error.to_string())?;
    let export_root = export_root_for_settings(app_root, &settings);
    command_state.last_export = match export_meeting_json_command(&store, meeting_id, &export_root)
    {
        Ok(exported) => Some(ExportCommandState::exported(exported)),
        Err(error) => Some(ExportCommandState::failed(meeting_id, error.to_string())),
    };
    drop(store);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn delete_meeting_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    command_state.last_delete = match delete_meeting_command(&store, meeting_id) {
        Ok(deleted) => Some(DeleteCommandState::deleted(deleted)),
        Err(error) => Some(DeleteCommandState::failed(meeting_id, error.to_string())),
    };
    drop(store);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn generate_summary_for_app_root(
    app_root: &Path,
    meeting_id: &str,
) -> Result<AnalysisCommandView, String> {
    let settings = app_settings_for_app_root(app_root)?;
    let client =
        LocalOllamaTextClient::new(settings.ollama_base_url.clone(), UreqOllamaHttpTransport);
    generate_summary_command_for_app_root_with_client(
        app_root,
        meeting_id,
        client,
        settings.ollama_model,
        current_timestamp_ms(),
    )
}

fn generate_summary_command_for_app_root_with_client<C>(
    app_root: &Path,
    meeting_id: &str,
    client: C,
    model_name: impl Into<String>,
    created_at_ms: u64,
) -> Result<AnalysisCommandView, String>
where
    C: ProviderTextClient,
{
    let store = open_store(app_root)?;
    let analyzer = OllamaAnalyzer::new(client, model_name, "summary-v1");
    generate_summary_command(&store, &analyzer, meeting_id, created_at_ms)
        .map(AnalysisCommandView::from_command)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
fn generate_summary_for_app_root_with_client<C>(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
    client: C,
    model_name: impl Into<String>,
    created_at_ms: u64,
) -> Result<DesktopSnapshot, String>
where
    C: ProviderTextClient,
{
    let command = generate_summary_command_for_app_root_with_client(
        app_root,
        meeting_id,
        client,
        model_name,
        created_at_ms,
    )?;
    command_state.last_analysis = Some(command);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn export_root_for_settings(app_root: &Path, settings: &AppSettings) -> PathBuf {
    settings
        .export_directory
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app_root.join("exports"))
}

#[cfg(any(test, debug_assertions))]
const DEV_FIXTURE_MEETING_ID: &str = "dev-fixture-meeting";
#[cfg(any(test, debug_assertions))]
const DEV_FIXTURE_TITLE: &str = "Dev Fixture Full Cycle";
#[cfg(any(test, debug_assertions))]
const DEV_FIXTURE_SESSION_ID: &str = "dev-fixture-session";
#[cfg(any(test, debug_assertions))]
const DEV_FIXTURE_ARTIFACT_ID: &str = "dev-fixture-artifact";
#[cfg(any(test, debug_assertions))]
const DEV_FIXTURE_ARTIFACT_PATH: &str = "meetings/dev-fixture-meeting/audio/imported.wav";
#[cfg(any(test, debug_assertions))]
const DEV_FIXTURE_AUDIO_SHA256: &str =
    "156075e2635b9b2c186258f4db987ed9fbdfb727f49e5eac4b9a126aefbdf727";

#[cfg(any(test, debug_assertions))]
fn seed_dev_fixture_for_app_root(
    app_root: &Path,
    command_state: &DesktopCommandState,
) -> Result<DesktopSnapshot, String> {
    seed_dev_fixture_rows(app_root)?;
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

#[cfg(any(test, debug_assertions))]
fn seed_dev_fixture_rows(app_root: &Path) -> Result<(), String> {
    let store = open_store(app_root)?;
    if store
        .list_meetings()
        .map_err(|error| error.to_string())?
        .iter()
        .any(|meeting| meeting.meeting_id == DEV_FIXTURE_MEETING_ID)
    {
        validate_existing_dev_fixture(&store, app_root)?;
        return Ok(());
    }
    if store
        .meeting_deleted(DEV_FIXTURE_MEETING_ID)
        .map_err(|error| error.to_string())?
    {
        return Err(
            "dev fixture was deleted in this app data store; reset the app data directory to seed it again"
                .to_string(),
        );
    }

    let meeting = Meeting::new_manual(DEV_FIXTURE_MEETING_ID, DEV_FIXTURE_TITLE, 1_700_000_000_000);
    store
        .insert_meeting(&meeting)
        .map_err(|error| error.to_string())?;
    let session = RecordingSession::start(
        DEV_FIXTURE_SESSION_ID,
        DEV_FIXTURE_MEETING_ID,
        RecordingSource::Imported,
        1_700_000_000_000,
        48_000,
    )
    .complete(1_700_000_003_000);
    store
        .insert_recording_session(&session)
        .map_err(|error| error.to_string())?;

    let absolute_artifact_path = app_root.join(DEV_FIXTURE_ARTIFACT_PATH);
    let parent = absolute_artifact_path
        .parent()
        .ok_or_else(|| "dev fixture artifact path has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    std::fs::write(&absolute_artifact_path, dev_fixture_wav_bytes())
        .map_err(|error| error.to_string())?;
    let inserted_artifact_id = store
        .insert_audio_artifact(&AudioArtifact::new_private(
            DEV_FIXTURE_ARTIFACT_ID,
            DEV_FIXTURE_SESSION_ID,
            ArtifactKind::Imported,
            DEV_FIXTURE_ARTIFACT_PATH,
            DEV_FIXTURE_AUDIO_SHA256,
        ))
        .map_err(|error| error.to_string())?;
    if inserted_artifact_id != DEV_FIXTURE_ARTIFACT_ID {
        return Err(format!(
            "dev fixture reused unexpected audio artifact: {inserted_artifact_id}"
        ));
    }
    store
        .complete_audio_artifact(DEV_FIXTURE_ARTIFACT_ID, DEV_FIXTURE_AUDIO_SHA256)
        .map_err(|error| error.to_string())?;
    store
        .update_meeting_status(
            DEV_FIXTURE_MEETING_ID,
            MeetingStatus::Complete,
            Some(1_700_000_003_000),
        )
        .map_err(|error| error.to_string())?;

    let run = ModelRun::new(
        "dev-fixture-run",
        DEV_FIXTURE_MEETING_ID,
        DEV_FIXTURE_AUDIO_SHA256,
        "fixture-local",
        "fixture-whisper",
        false,
        1_700_000_003_100,
    );
    let version = TranscriptVersion::new(
        "dev-fixture-version",
        DEV_FIXTURE_MEETING_ID,
        "dev-fixture-run",
        1,
        1_700_000_003_200,
    );
    let segments = [
        TranscriptSegment::with_metadata(
            "dev-fixture-segment-1",
            DEV_FIXTURE_MEETING_ID,
            0,
            1_500,
            "Dev fixture kickoff covers deterministic transcript search.",
            SourceChannel::Imported,
            &run.id,
            &version.id,
        ),
        TranscriptSegment::with_metadata(
            "dev-fixture-segment-2",
            DEV_FIXTURE_MEETING_ID,
            1_500,
            3_000,
            "Export delete and local summary generation should work without live hardware.",
            SourceChannel::Imported,
            &run.id,
            &version.id,
        ),
    ];
    store
        .persist_transcript(&run, &version, &segments)
        .map_err(|error| error.to_string())
}

#[cfg(any(test, debug_assertions))]
fn validate_existing_dev_fixture(store: &Store, app_root: &Path) -> Result<(), String> {
    let status = store
        .meeting_status(DEV_FIXTURE_MEETING_ID)
        .map_err(|error| error.to_string())?;
    if status != "Complete" {
        return Err(format!(
            "partial dev fixture: expected Complete meeting status, got {status}"
        ));
    }
    let artifact = store
        .completed_wav_artifact_for_transcription(DEV_FIXTURE_MEETING_ID)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "partial dev fixture: missing completed private audio artifact".to_string()
        })?;
    if artifact.artifact_id != DEV_FIXTURE_ARTIFACT_ID
        || artifact.recording_session_id != DEV_FIXTURE_SESSION_ID
        || artifact.sha256 != DEV_FIXTURE_AUDIO_SHA256
    {
        return Err("partial dev fixture: completed audio artifact identity changed".to_string());
    }
    if artifact.path != DEV_FIXTURE_ARTIFACT_PATH {
        return Err("partial dev fixture: completed audio artifact path changed".to_string());
    }
    if !app_root.join(&artifact.path).is_file() {
        return Err(
            "partial dev fixture: completed private audio artifact file is missing".to_string(),
        );
    }
    let segments = store
        .transcript_segments(DEV_FIXTURE_MEETING_ID)
        .map_err(|error| error.to_string())?;
    if segments.len() != 2 {
        return Err(format!(
            "partial dev fixture: expected 2 transcript segments, got {}",
            segments.len()
        ));
    }
    Ok(())
}

#[cfg(any(test, debug_assertions))]
fn dev_fixture_wav_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&38u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    bytes
}

#[derive(Default)]
struct DesktopCommandState {
    active_recording: Option<ActiveDesktopRecording>,
    starting_recording: bool,
    last_recording: Option<CommandRecordingDto>,
    last_transcription: Option<TranscriptionCommandView>,
    last_export: Option<ExportCommandState>,
    last_delete: Option<DeleteCommandState>,
    last_analysis: Option<AnalysisCommandView>,
}

impl DesktopCommandState {
    fn snapshot_state(&self) -> DesktopCommandSnapshotState {
        DesktopCommandSnapshotState {
            active_recording: self.active_recording.as_ref().map(|recording| {
                ActiveDesktopRecordingSnapshot {
                    meeting_id: recording.meeting_id.clone(),
                    recording_id: recording.recording_id.clone(),
                    captures_system_audio: recording.streams.contains(&StreamKind::SystemAudio),
                }
            }),
            last_recording: self.last_recording.clone(),
            last_transcription: self.last_transcription.clone(),
            last_export: self.last_export.clone(),
            last_delete: self.last_delete.clone(),
            last_analysis: self.last_analysis.clone(),
        }
    }
}

#[derive(Clone, Default)]
struct DesktopCommandSnapshotState {
    active_recording: Option<ActiveDesktopRecordingSnapshot>,
    last_recording: Option<CommandRecordingDto>,
    last_transcription: Option<TranscriptionCommandView>,
    last_export: Option<ExportCommandState>,
    last_delete: Option<DeleteCommandState>,
    last_analysis: Option<AnalysisCommandView>,
}

#[derive(Clone)]
struct ActiveDesktopRecordingSnapshot {
    meeting_id: String,
    recording_id: String,
    captures_system_audio: bool,
}

struct ActiveDesktopRecording {
    meeting_id: String,
    recording_id: String,
    streams: Vec<StreamKind>,
    recorder: Box<dyn ActiveMicrophoneRecording>,
}

struct StartedMicrophoneRecording {
    sample_rate_hz: u32,
    streams: Vec<StreamKind>,
    recorder: Box<dyn ActiveMicrophoneRecording>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MicrophoneStartFailure {
    permission_state: AppPermissionState,
    message: String,
    recovery_action: String,
}

impl MicrophoneStartFailure {
    fn persistence(message: impl Into<String>) -> Self {
        Self {
            permission_state: AppPermissionState::MicrophoneUnavailable,
            message: message.into(),
            recovery_action: "Check local storage permissions and retry microphone recording."
                .to_string(),
        }
    }

    #[cfg(test)]
    fn permission_denied(message: impl Into<String>) -> Self {
        Self {
            permission_state: AppPermissionState::MicrophoneDenied,
            message: message.into(),
            recovery_action:
                "Open System Settings; go to Privacy & Security, then Microphone; allow Curiosity Transcripts and retry recording."
                    .to_string(),
        }
    }

    fn from_capture_error(error: CaptureError) -> Self {
        match error {
            CaptureError::PermissionDenied(error) => {
                let permission_state = match error.permission {
                    CapturePermission::Microphone => AppPermissionState::MicrophoneDenied,
                    CapturePermission::SystemAudioScreenRecording => {
                        AppPermissionState::SystemAudioDenied
                    }
                };
                let guidance = error.recovery_guidance();
                Self {
                    permission_state,
                    message: error.to_string(),
                    recovery_action: guidance.steps.join("; "),
                }
            }
            CaptureError::Unavailable(error) => {
                let permission_state = match error.capability {
                    CaptureCapability::Microphone => AppPermissionState::MicrophoneUnavailable,
                    CaptureCapability::SystemAudio => AppPermissionState::SystemAudioUnavailable,
                };
                let guidance = error.recovery_guidance();
                Self {
                    permission_state,
                    message: error.to_string(),
                    recovery_action: guidance.steps.join("; "),
                }
            }
            CaptureError::Configuration(error) => Self {
                permission_state: AppPermissionState::MicrophoneUnavailable,
                message: error.to_string(),
                recovery_action: "Check the microphone capture configuration and retry recording."
                    .to_string(),
            },
            CaptureError::Recording(error) => Self {
                permission_state: AppPermissionState::MicrophoneUnavailable,
                message: error.to_string(),
                recovery_action:
                    "Check local storage and microphone availability, then retry recording."
                        .to_string(),
            },
        }
    }
}

trait MicrophoneRecorderFactory {
    fn start(
        &self,
        audio_root: &Path,
        recording_id: &str,
        started_at_ms: u64,
    ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure>;
}

trait ActiveMicrophoneRecording: Send {
    fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String>;
}

struct RealMicrophoneRecorderFactory;

impl MicrophoneRecorderFactory for RealMicrophoneRecorderFactory {
    fn start(
        &self,
        audio_root: &Path,
        recording_id: &str,
        started_at_ms: u64,
    ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
        match MacosDesktopWavRecording::start(audio_root, recording_id, started_at_ms) {
            Ok(recorder) => {
                let sample_rate_hz = recorder.sample_rate_hz();
                Ok(StartedMicrophoneRecording {
                    sample_rate_hz,
                    streams: vec![StreamKind::Microphone, StreamKind::SystemAudio],
                    recorder: Box::new(recorder),
                })
            }
            Err(error) if can_fallback_to_microphone_recording(&error) => {
                let recorder =
                    MacosMicrophoneWavRecording::start(audio_root, recording_id, started_at_ms)
                        .map_err(MicrophoneStartFailure::from_capture_error)?;
                let sample_rate_hz = recorder.sample_rate_hz();
                Ok(StartedMicrophoneRecording {
                    sample_rate_hz,
                    streams: vec![StreamKind::Microphone],
                    recorder: Box::new(recorder),
                })
            }
            Err(error) => Err(MicrophoneStartFailure::from_capture_error(error)),
        }
    }
}

fn can_fallback_to_microphone_recording(error: &CaptureError) -> bool {
    matches!(
        error,
        CaptureError::Unavailable(unavailable)
            if unavailable.capability == CaptureCapability::SystemAudio
    ) || matches!(
        error,
        CaptureError::PermissionDenied(permission)
            if permission.permission == CapturePermission::SystemAudioScreenRecording
    )
}

impl ActiveMicrophoneRecording for MacosDesktopWavRecording {
    fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
        (*self).stop(ended_at_ms).map_err(|error| error.to_string())
    }
}

impl ActiveMicrophoneRecording for MacosMicrophoneWavRecording {
    fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
        (*self).stop(ended_at_ms).map_err(|error| error.to_string())
    }
}

fn start_microphone_recording_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    title: Option<String>,
    started_at_ms: u64,
    factory: &impl MicrophoneRecorderFactory,
) -> Result<DesktopSnapshot, MicrophoneStartFailure> {
    if command_state.active_recording.is_some() {
        return Err(MicrophoneStartFailure::persistence(
            "Stop the active recording before starting another one.",
        ));
    }

    let store = open_store(app_root).map_err(MicrophoneStartFailure::persistence)?;
    let meeting_id = format!("meeting-{started_at_ms}");
    let recording_id = format!("recording-{started_at_ms}");
    let title = title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Untitled recording".to_string());
    let audio_root = app_root.join("meetings").join(&meeting_id).join("audio");
    let StartedMicrophoneRecording {
        sample_rate_hz,
        streams,
        recorder,
    } = factory.start(&audio_root, &recording_id, started_at_ms)?;

    let mut meeting = Meeting::new_manual(&meeting_id, title, started_at_ms);
    let session = RecordingSession::start(
        &recording_id,
        &meeting_id,
        required_recording_source_for_streams(&streams),
        started_at_ms,
        sample_rate_hz,
    );
    meeting.start_recording(&session);
    let artifacts = audio_artifacts_for_streams(&meeting_id, &recording_id, &streams);

    if let Err(error) = store.insert_recording_start_with_artifacts(&meeting, &session, &artifacts)
    {
        return Err(metadata_persistence_failure(
            error.to_string(),
            recorder,
            started_at_ms,
            audio_root.join(&recording_id),
        ));
    }
    let recoverable_artifacts = artifacts
        .iter()
        .map(|artifact| RecoverableArtifact {
            artifact_id: artifact.id.clone(),
            path: artifact.path.clone(),
            sha256: artifact.sha256.clone(),
        })
        .collect::<Vec<_>>();
    if let Err(error) = store.write_recoverable_artifact_manifests(
        &meeting_id,
        &recording_id,
        &recoverable_artifacts,
    ) {
        return Err(metadata_persistence_failure(
            error.to_string(),
            recorder,
            started_at_ms,
            audio_root.join(&recording_id),
        ));
    }

    let recording = recording_dto(
        &meeting_id,
        Some(recording_id.clone()),
        CommandRecordingState::Recording,
        AppPermissionState::Ready,
        microphone_storage_path(&meeting_id),
        "Recording locally to private app storage",
    );
    command_state.active_recording = Some(ActiveDesktopRecording {
        meeting_id,
        recording_id,
        streams,
        recorder,
    });
    command_state.last_recording = Some(recording);
    command_state.last_transcription = None;

    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
        .map_err(MicrophoneStartFailure::persistence)
}

fn metadata_persistence_failure(
    error: String,
    recorder: Box<dyn ActiveMicrophoneRecording>,
    ended_at_ms: u64,
    session_dir: PathBuf,
) -> MicrophoneStartFailure {
    let stop_error = recorder.stop(ended_at_ms).err();
    let remove_error = if session_dir.exists() {
        std::fs::remove_dir_all(&session_dir).err()
    } else {
        None
    };
    let mut message = format!("Recording metadata could not be persisted: {error}");
    if let Some(stop_error) = stop_error {
        message.push_str(&format!(". Cleanup stop also failed: {stop_error}"));
    }
    if let Some(remove_error) = remove_error {
        message.push_str(&format!(
            ". Cleanup could not remove {}: {remove_error}",
            session_dir.display()
        ));
    }
    MicrophoneStartFailure::persistence(message)
}

#[cfg(test)]
fn stop_microphone_recording_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    ended_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let Some(active) = command_state.active_recording.take() else {
        return Err("Start a desktop recording before stopping.".to_string());
    };
    command_state.last_recording = Some(stop_active_microphone_recording(
        app_root,
        active,
        ended_at_ms,
    ));

    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn stop_active_microphone_recording(
    app_root: &Path,
    active: ActiveDesktopRecording,
    ended_at_ms: u64,
) -> CommandRecordingDto {
    let meeting_id = active.meeting_id.clone();
    let recording_id = active.recording_id.clone();
    match complete_active_microphone_recording(app_root, active, ended_at_ms) {
        Ok(recording) => recording,
        Err(message) => {
            if let Ok(store) = open_store(app_root) {
                let _ = store.update_recording_session_status(
                    &recording_id,
                    RecordingStatus::Failed,
                    Some(ended_at_ms),
                    Some(&message),
                );
                let _ = store.update_meeting_status(
                    &meeting_id,
                    MeetingStatus::Failed,
                    Some(ended_at_ms),
                );
            }
            recording_dto(
                &meeting_id,
                Some(recording_id),
                CommandRecordingState::Interrupted,
                recording_stop_permission_state(&message),
                microphone_storage_path(&meeting_id),
                &format!("Recording could not be finalized: {message}"),
            )
        }
    }
}

fn recording_stop_permission_state(message: &str) -> AppPermissionState {
    if message.to_ascii_lowercase().contains("system audio") {
        AppPermissionState::SystemAudioUnavailable
    } else {
        AppPermissionState::MicrophoneUnavailable
    }
}

fn complete_active_microphone_recording(
    app_root: &Path,
    active: ActiveDesktopRecording,
    ended_at_ms: u64,
) -> Result<CommandRecordingDto, String> {
    let manifest = active.recorder.stop(ended_at_ms)?;
    let store = open_store(app_root)?;
    let mut completed_artifacts = Vec::new();
    let mut completed_streams = Vec::new();
    for artifact in &manifest.artifacts {
        if !active.streams.contains(&artifact.stream) {
            return Err(format!(
                "{} artifact was not part of the active recording",
                stream_label(artifact.stream)
            ));
        }
        let relative_path = artifact
            .path
            .strip_prefix(app_root)
            .map_err(|_| {
                format!(
                    "{} artifact was written outside private app storage",
                    stream_label(artifact.stream)
                )
            })?
            .to_string_lossy()
            .to_string();
        let expected_path = artifact_relative_path_for_stream(
            &active.meeting_id,
            &active.recording_id,
            artifact.stream,
        );
        if relative_path != expected_path {
            return Err(format!(
                "{} artifact path mismatch: expected {expected_path}, got {relative_path}",
                stream_label(artifact.stream)
            ));
        }
        completed_streams.push(artifact.stream);
        completed_artifacts.push(CompletedAudioArtifact {
            artifact_id: artifact_id_for_stream(&active.recording_id, artifact.stream),
            sha256: artifact.sha256.clone(),
        });
    }
    if !completed_streams.contains(&StreamKind::Microphone) {
        return Err("microphone recording stopped without a WAV artifact".to_string());
    }
    let recording_source = recording_source_for_streams(&completed_streams);
    store
        .complete_recording_session_with_artifacts(
            &active.meeting_id,
            &active.recording_id,
            ended_at_ms,
            recording_source,
            &completed_artifacts,
        )
        .map_err(|error| error.to_string())?;

    let recovery_action = if completed_streams.contains(&StreamKind::SystemAudio) {
        "Finalized local microphone and system audio WAV artifacts."
    } else {
        "Finalized local microphone WAV artifact."
    };

    Ok(recording_dto(
        &active.meeting_id,
        Some(active.recording_id),
        CommandRecordingState::Complete,
        AppPermissionState::Ready,
        microphone_storage_path(&active.meeting_id),
        recovery_action,
    ))
}

#[cfg(test)]
fn transcribe_meeting_for_app_root<B: WhisperBackend>(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
    model_path: impl Into<PathBuf>,
    model_name: impl Into<String>,
    backend: B,
    created_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    command_state.last_transcription = Some(transcribe_meeting_command(
        app_root,
        meeting_id,
        model_path,
        model_name,
        backend,
        created_at_ms,
    )?);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn transcribe_meeting_command<B: WhisperBackend>(
    app_root: &Path,
    meeting_id: &str,
    model_path: impl Into<PathBuf>,
    model_name: impl Into<String>,
    backend: B,
    created_at_ms: u64,
) -> Result<TranscriptionCommandView, String> {
    let model_path = model_path.into();
    let model_name = model_name.into();
    let store = open_store(app_root)?;
    let artifacts = store
        .completed_wav_artifacts_for_transcription(meeting_id)
        .map_err(|error| error.to_string())?;
    if artifacts.is_empty() {
        return Ok(transcription_failed(
            meeting_id,
            "missing_audio_artifact",
            "No completed retained local WAV artifact exists for this meeting.",
            "Stop a desktop recording before requesting transcription.",
        ));
    }

    let requests = artifacts
        .iter()
        .map(|artifact| {
            WhisperTranscriptionRequest::new(
                meeting_id,
                app_root.join(&artifact.path),
                artifact.sha256.clone(),
                source_channel_for_artifact_kind(&artifact.kind),
            )
        })
        .collect::<Vec<_>>();
    let transcriber = WhisperTranscriber::new(model_path, model_name, backend);
    match transcriber.transcribe_wav_bundle(&requests) {
        Ok(document) => {
            match persist_transcription_document(&store, meeting_id, document, created_at_ms) {
                Ok(()) => Ok(TranscriptionCommandView {
                    meeting_id: meeting_id.to_string(),
                    state: TranscriptionCommandState::Complete,
                    failure: None,
                }),
                Err(error) => Ok(transcription_failed(
                    meeting_id,
                    "transcript_persist_failed",
                    &format!("Transcription completed but could not be saved: {error}"),
                    "Check local app storage and retry transcription.",
                )),
            }
        }
        Err(error) => Ok(transcription_failure_from_error(meeting_id, error)),
    }
}

fn persist_transcription_document(
    store: &Store,
    meeting_id: &str,
    document: TranscriptionDocument,
    created_at_ms: u64,
) -> curiosity_store::StoreResult<()> {
    let run = ModelRun::new(
        document.model_run_id.clone(),
        meeting_id,
        document.source_artifact_sha256,
        document.provider,
        document.model_name,
        false,
        created_at_ms,
    );
    let version = TranscriptVersion::new(
        document.transcript_version_id,
        run.meeting_id.clone(),
        run.id.clone(),
        1,
        created_at_ms,
    );
    store.persist_transcript(&run, &version, &document.segments)
}

fn source_channel_for_artifact_kind(kind: &str) -> SourceChannel {
    match kind {
        "RawMic" => SourceChannel::Microphone,
        "RawSystem" => SourceChannel::System,
        "Mixed" => SourceChannel::Mixed,
        _ => SourceChannel::Imported,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg(not(feature = "whisper-rs"))]
struct BackendUnavailableWhisperBackend;

#[cfg(not(feature = "whisper-rs"))]
impl WhisperBackend for BackendUnavailableWhisperBackend {
    fn provider(&self) -> &'static str {
        "local-whisper"
    }

    fn transcribe(
        &self,
        _model_path: &Path,
        _audio_path: &Path,
    ) -> Result<Vec<curiosity_transcription::WhisperBackendSegment>, TranscriptionError> {
        Err(TranscriptionError::BackendUnavailable {
            provider: self.provider().to_string(),
            guidance: "Build the desktop backend with the whisper-rs feature to run local Whisper transcription."
                .to_string(),
        })
    }
}

fn transcription_failure_from_error(
    meeting_id: &str,
    error: TranscriptionError,
) -> TranscriptionCommandView {
    match error {
        TranscriptionError::MissingModelPath { guidance, .. } => transcription_failed(
            meeting_id,
            "missing_model",
            &format!("Whisper model is unavailable. {guidance}"),
            &guidance,
        ),
        TranscriptionError::EmptyAudioInput { guidance } => transcription_failed(
            meeting_id,
            "missing_audio",
            &format!("Audio input is unavailable. {guidance}"),
            &guidance,
        ),
        TranscriptionError::AudioInputUnavailable { guidance, .. } => transcription_failed(
            meeting_id,
            "missing_audio",
            &format!("Audio input is unavailable. {guidance}"),
            &guidance,
        ),
        TranscriptionError::UnsupportedAudioInput { guidance, .. } => transcription_failed(
            meeting_id,
            "unsupported_audio",
            &format!("Audio input is unsupported. {guidance}"),
            &guidance,
        ),
        TranscriptionError::BackendUnavailable { guidance, .. } => transcription_failed(
            meeting_id,
            "backend_unavailable",
            &format!("Local Whisper backend is unavailable. {guidance}"),
            &guidance,
        ),
        TranscriptionError::BackendFailed { message, .. } => transcription_failed(
            meeting_id,
            "backend_failed",
            &message,
            "Check the local Whisper model and WAV artifact, then retry transcription.",
        ),
    }
}

fn transcription_failed(
    meeting_id: &str,
    code: &str,
    message: &str,
    setup_guidance: &str,
) -> TranscriptionCommandView {
    TranscriptionCommandView {
        meeting_id: meeting_id.to_string(),
        state: TranscriptionCommandState::Failed,
        failure: Some(CommandFailureView {
            code: code.to_string(),
            message: message.to_string(),
            setup_guidance: setup_guidance.to_string(),
        }),
    }
}

fn recording_snapshot(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
) -> CommandRecordingDto {
    if let Some(active) = &command_state.active_recording {
        return recording_dto(
            &active.meeting_id,
            Some(active.recording_id.clone()),
            CommandRecordingState::Recording,
            AppPermissionState::Ready,
            microphone_storage_path(&active.meeting_id),
            "Recording locally to private app storage",
        );
    }
    if let Some(recording) = &command_state.last_recording {
        return recording.clone();
    }
    recording_dto(
        "",
        None,
        CommandRecordingState::Idle,
        AppPermissionState::Ready,
        app_root.display().to_string(),
        "Start a desktop recording to create private microphone and system audio WAV artifacts.",
    )
}

fn microphone_capture_state(command_state: &DesktopCommandSnapshotState) -> DesktopPermissionState {
    if command_state.active_recording.is_some() {
        return DesktopPermissionState::Ready;
    }
    if let Some(recording) = &command_state.last_recording {
        return match recording.permission_state {
            AppPermissionState::Ready => DesktopPermissionState::Ready,
            AppPermissionState::MicrophoneDenied => DesktopPermissionState::MicrophoneDenied,
            AppPermissionState::MicrophoneUnavailable => {
                DesktopPermissionState::MicrophoneUnavailable
            }
            AppPermissionState::SystemAudioDenied | AppPermissionState::SystemAudioUnavailable => {
                DesktopPermissionState::Ready
            }
        };
    }
    DesktopPermissionState::Ready
}

fn system_audio_capture_state(
    command_state: &DesktopCommandSnapshotState,
) -> DesktopPermissionState {
    if command_state
        .active_recording
        .as_ref()
        .map(|recording| recording.captures_system_audio)
        .unwrap_or(false)
    {
        return DesktopPermissionState::Ready;
    }
    if let Some(recording) = &command_state.last_recording {
        match recording.permission_state {
            AppPermissionState::SystemAudioDenied => {
                return DesktopPermissionState::SystemAudioDenied
            }
            AppPermissionState::SystemAudioUnavailable => {
                return DesktopPermissionState::SystemAudioUnavailable;
            }
            AppPermissionState::Ready => return DesktopPermissionState::Ready,
            AppPermissionState::MicrophoneDenied | AppPermissionState::MicrophoneUnavailable => {}
        }
    }
    #[cfg(test)]
    {
        DesktopPermissionState::SystemAudioUnavailable
    }
    #[cfg(not(test))]
    match ScreenCaptureKitSystemAudioAdapter::status() {
        SystemAudioAdapterStatus::Available => DesktopPermissionState::Ready,
        SystemAudioAdapterStatus::PermissionDenied(_) => DesktopPermissionState::SystemAudioDenied,
        SystemAudioAdapterStatus::Unavailable(_) => DesktopPermissionState::SystemAudioUnavailable,
    }
}

fn start_failure_recording_dto(
    app_root: &Path,
    error: &MicrophoneStartFailure,
) -> CommandRecordingDto {
    recording_dto(
        "",
        None,
        CommandRecordingState::Interrupted,
        error.permission_state,
        app_root.display().to_string(),
        &format!(
            "Desktop recording could not start: {} {}",
            error.message, error.recovery_action
        ),
    )
}

fn recording_dto(
    meeting_id: &str,
    recording_id: Option<String>,
    state: CommandRecordingState,
    permission_state: AppPermissionState,
    storage_path: String,
    recovery_action: &str,
) -> CommandRecordingDto {
    CommandRecordingDto {
        meeting_id: meeting_id.to_string(),
        recording_id,
        state,
        permission_state,
        storage_location: StorageLocationDto {
            app_private_path: storage_path,
        },
        raw_audio_retention: RawAudioRetentionPolicy::Retain,
        recoverable: false,
        recovery_action: recovery_action.to_string(),
    }
}

fn artifact_id(recording_id: &str) -> String {
    format!("artifact-{recording_id}")
}

fn system_audio_artifact_id(recording_id: &str) -> String {
    format!("artifact-{recording_id}-system")
}

fn artifact_id_for_stream(recording_id: &str, stream: StreamKind) -> String {
    match stream {
        StreamKind::Microphone => artifact_id(recording_id),
        StreamKind::SystemAudio => system_audio_artifact_id(recording_id),
    }
}

fn stream_label(stream: StreamKind) -> &'static str {
    match stream {
        StreamKind::Microphone => "microphone",
        StreamKind::SystemAudio => "system audio",
    }
}

fn recording_source_for_streams(streams: &[StreamKind]) -> RecordingSource {
    let has_microphone = streams.contains(&StreamKind::Microphone);
    let has_system_audio = streams.contains(&StreamKind::SystemAudio);
    match (has_microphone, has_system_audio) {
        (true, true) => RecordingSource::Mixed,
        (false, true) => RecordingSource::System,
        _ => RecordingSource::Microphone,
    }
}

fn required_recording_source_for_streams(streams: &[StreamKind]) -> RecordingSource {
    if streams.contains(&StreamKind::Microphone) {
        RecordingSource::Microphone
    } else {
        recording_source_for_streams(streams)
    }
}

fn audio_artifacts_for_streams(
    meeting_id: &str,
    recording_id: &str,
    streams: &[StreamKind],
) -> Vec<AudioArtifact> {
    streams
        .iter()
        .map(|stream| match stream {
            StreamKind::Microphone => AudioArtifact::new_private(
                artifact_id(recording_id),
                recording_id,
                ArtifactKind::RawMic,
                microphone_artifact_relative_path(meeting_id, recording_id),
                format!("sha256:pending:{}", artifact_id(recording_id)),
            ),
            StreamKind::SystemAudio => AudioArtifact::new_private(
                system_audio_artifact_id(recording_id),
                recording_id,
                ArtifactKind::RawSystem,
                system_audio_artifact_relative_path(meeting_id, recording_id),
                format!("sha256:pending:{}", system_audio_artifact_id(recording_id)),
            ),
        })
        .collect()
}

fn microphone_storage_path(meeting_id: &str) -> String {
    format!("meetings/{meeting_id}/audio")
}

fn microphone_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/raw-mic.wav",
        microphone_storage_path(meeting_id)
    )
}

fn system_audio_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/raw-system.wav",
        microphone_storage_path(meeting_id)
    )
}

fn artifact_relative_path_for_stream(
    meeting_id: &str,
    recording_id: &str,
    stream: StreamKind,
) -> String {
    match stream {
        StreamKind::Microphone => microphone_artifact_relative_path(meeting_id, recording_id),
        StreamKind::SystemAudio => system_audio_artifact_relative_path(meeting_id, recording_id),
    }
}

fn model_status_from_settings(settings: &AppSettings) -> ModelStatus {
    let configured_path = resolved_whisper_model_path(settings);
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

fn resolved_whisper_model_path(settings: &AppSettings) -> String {
    let saved_path = settings.whisper_model_path.trim();
    if saved_path.is_empty() {
        std::env::var("CURIOSITY_WHISPER_MODEL").unwrap_or_default()
    } else {
        saved_path.to_string()
    }
}

fn model_name_for_path(model_path: &str) -> String {
    PathBuf::from(model_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-whisper")
        .to_string()
}

fn app_settings_view(settings: AppSettings) -> AppSettingsView {
    AppSettingsView {
        whisper_model_path: settings.whisper_model_path,
        ollama_base_url: settings.ollama_base_url,
        ollama_model: settings.ollama_model,
        export_directory: settings.export_directory,
    }
}

fn test_whisper_model_path_value(path: &str) -> WhisperModelPathTestView {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return WhisperModelPathTestView::invalid(
            "No Whisper model path is configured.",
            "Enter a local Whisper model path, or set CURIOSITY_WHISPER_MODEL before launching the app.",
        );
    }
    let path = PathBuf::from(trimmed);
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return WhisperModelPathTestView::invalid(
                format!("Whisper model path does not exist or cannot be inspected: {error}"),
                "Check the path and choose a readable local Whisper model file.",
            );
        }
    };
    if !metadata.is_file() {
        return WhisperModelPathTestView::invalid(
            "Whisper model path must point to a file.",
            "Choose a readable local Whisper model file, not a directory.",
        );
    }
    match std::fs::File::open(&path) {
        Ok(_) => WhisperModelPathTestView {
            state: "Valid".to_string(),
            message: "Whisper model path is readable.".to_string(),
            setup_guidance:
                "Save this path, then transcribe with the whisper-rs desktop feature enabled."
                    .to_string(),
        },
        Err(error) => WhisperModelPathTestView::invalid(
            format!("Whisper model path is not readable: {error}"),
            "Check file permissions and choose a readable local Whisper model file.",
        ),
    }
}

#[derive(Clone)]
struct LocalOllamaTextClient<T> {
    base_url: String,
    transport: T,
}

impl<T> LocalOllamaTextClient<T> {
    fn new(base_url: impl Into<String>, transport: T) -> Self {
        Self {
            base_url: base_url.into(),
            transport,
        }
    }
}

impl<T> ProviderTextClient for LocalOllamaTextClient<T>
where
    T: OllamaHttpTransport,
{
    fn complete(&self, model_name: &str, prompt: &str) -> Result<String, AnalysisClientError> {
        validate_local_ollama_model(model_name)?;
        let url = local_ollama_endpoint(&self.base_url, "/api/generate")?;
        let body = serde_json::json!({
            "model": model_name,
            "prompt": prompt,
            "stream": false,
            "format": summary_json_schema(),
            "options": {
                "temperature": 0,
                "top_p": 0.1,
                "seed": 1,
            },
        });
        let response = self
            .transport
            .post_json(&url, body)
            .map_err(AnalysisClientError::from)?;
        response
            .get("response")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                AnalysisClientError::Transport(
                    "Ollama /api/generate response did not include a response string.".to_string(),
                )
            })
    }
}

trait OllamaHttpTransport {
    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, OllamaHttpError>;
    fn get_json(&self, url: &str) -> Result<serde_json::Value, OllamaHttpError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OllamaHttpError {
    Unavailable(String),
    Http { status: u16, body: String },
    MalformedResponse(String),
}

impl std::fmt::Display for OllamaHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::MalformedResponse(message) => write!(f, "{message}"),
            Self::Http { status, body } => write!(f, "Ollama returned HTTP {status}: {body}"),
        }
    }
}

impl From<OllamaHttpError> for AnalysisClientError {
    fn from(error: OllamaHttpError) -> Self {
        match error {
            OllamaHttpError::Unavailable(message) => Self::Unavailable(message),
            OllamaHttpError::Http { .. } | OllamaHttpError::MalformedResponse(_) => {
                Self::Transport(error.to_string())
            }
        }
    }
}

#[derive(Clone, Copy)]
struct UreqOllamaHttpTransport;

impl OllamaHttpTransport for UreqOllamaHttpTransport {
    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, OllamaHttpError> {
        ollama_ureq_agent()
            .post(url)
            .send_json(body)
            .map_err(ollama_http_error_from_ureq)?
            .into_json()
            .map_err(|error| {
                OllamaHttpError::MalformedResponse(format!("parse Ollama response JSON: {error}"))
            })
    }

    fn get_json(&self, url: &str) -> Result<serde_json::Value, OllamaHttpError> {
        ollama_ureq_agent()
            .get(url)
            .call()
            .map_err(ollama_http_error_from_ureq)?
            .into_json()
            .map_err(|error| {
                OllamaHttpError::MalformedResponse(format!("parse Ollama response JSON: {error}"))
            })
    }
}

fn ollama_ureq_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_write(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(120))
        .build()
}

fn ollama_http_error_from_ureq(error: ureq::Error) -> OllamaHttpError {
    match error {
        ureq::Error::Status(code, response) => {
            let status_text = response.status_text().to_string();
            let body = response.into_string().unwrap_or_else(|error| {
                format!("{status_text}; read response body failed: {error}")
            });
            let body = body.trim();
            OllamaHttpError::Http {
                status: code,
                body: if body.is_empty() {
                    status_text
                } else {
                    body.to_string()
                },
            }
        }
        ureq::Error::Transport(error) => OllamaHttpError::Unavailable(error.to_string()),
    }
}

fn test_ollama_connection_value<T>(
    base_url: &str,
    model_name: &str,
    transport: &T,
) -> OllamaConnectionTestView
where
    T: OllamaHttpTransport,
{
    if let Err(error) = validate_local_ollama_model(model_name) {
        return OllamaConnectionTestView::unavailable(
            error.to_string(),
            "Choose a local Ollama model tag such as qwen3.6:27b or gemma4:31b.",
        );
    }
    let url = match local_ollama_endpoint(base_url, "/api/tags") {
        Ok(url) => url,
        Err(error) => {
            return OllamaConnectionTestView::unavailable(
                error.to_string(),
                "Use a local Ollama base URL such as http://127.0.0.1:11434.",
            );
        }
    };
    let response = match transport.get_json(&url) {
        Ok(response) => response,
        Err(error) => {
            return OllamaConnectionTestView::unavailable(
                format!("Ollama is unavailable: {error}"),
                "Start Ollama with `ollama serve`, then retry.",
            );
        }
    };
    let installed_models = installed_ollama_model_names(&response);
    let installed = installed_models
        .iter()
        .any(|installed_model| ollama_model_matches_request(installed_model, model_name));
    if installed {
        OllamaConnectionTestView {
            state: "Available".to_string(),
            message: format!("Ollama is reachable and {model_name} is installed."),
            setup_guidance: String::new(),
        }
    } else {
        let installed_hint = if installed_models.is_empty() {
            " No local models were reported by Ollama.".to_string()
        } else {
            format!(" Installed local models: {}.", installed_models.join(", "))
        };
        OllamaConnectionTestView::unavailable(
            format!("Ollama is reachable, but {model_name} is not installed."),
            format!(
                "Install the selected model with `ollama pull {model_name}`, then retry.{installed_hint}"
            ),
        )
    }
}

fn installed_ollama_model_names(response: &serde_json::Value) -> Vec<String> {
    let mut names = response
        .get("models")
        .and_then(|models| models.as_array())
        .into_iter()
        .flatten()
        .flat_map(|model| {
            ["name", "model"].into_iter().filter_map(|field| {
                model
                    .get(field)
                    .and_then(|name| name.as_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
            })
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn ollama_model_matches_request(installed_model: &str, requested_model: &str) -> bool {
    let installed_model = installed_model.trim();
    let requested_model = requested_model.trim();
    installed_model == requested_model
        || (!requested_model.contains(':')
            && installed_model == format!("{requested_model}:latest"))
}

fn validate_local_ollama_model(model_name: &str) -> Result<(), AnalysisClientError> {
    let trimmed = model_name.trim();
    if trimmed.is_empty() {
        return Err(AnalysisClientError::Transport(
            "Choose a local Ollama model before requesting analysis.".to_string(),
        ));
    }
    let is_hosted = trimmed.ends_with(":cloud")
        || recommended_analysis_model_presets().iter().any(|preset| {
            preset.provider_kind != AnalysisProviderKind::OllamaLocal
                && (preset.model_tag == trimmed || preset.id == trimmed)
        });
    if is_hosted {
        return Err(AnalysisClientError::Transport(
            "hosted or cloud model tags cannot use the local Ollama privacy path.".to_string(),
        ));
    }
    Ok(())
}

fn local_ollama_endpoint(base_url: &str, path: &str) -> Result<String, AnalysisClientError> {
    let mut url = Url::parse(base_url.trim()).map_err(|error| {
        AnalysisClientError::Transport(format!("Ollama base URL is invalid: {error}"))
    })?;
    let is_loopback = match url.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false),
        None => false,
    };
    if !is_loopback {
        return Err(AnalysisClientError::Transport(
            "Ollama base URL must use localhost or a loopback IP address for local analysis."
                .to_string(),
        ));
    }
    match url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(AnalysisClientError::Transport(
                "Ollama base URL must use http or https.".to_string(),
            ))
        }
    }
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
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

fn system_audio_smoke_recording_for_app_root(
    app_root: &Path,
    duration: std::time::Duration,
) -> CaptureProbeStatus {
    let output_root = app_root.join("system-audio-smoke");
    smoke_result_status(
        ManualSmokeCheck::macos_placeholder()
            .run_macos_system_audio_capture(&output_root, duration),
    )
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
    settings: AppSettingsView,
    capture: CaptureStatus,
    transcription: Option<TranscriptionCommandView>,
    export_command: ExportCommandState,
    delete_command: DeleteCommandState,
    analysis_command: Option<AnalysisCommandView>,
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
struct AppSettingsView {
    whisper_model_path: String,
    ollama_base_url: String,
    ollama_model: String,
    export_directory: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WhisperModelPathTestView {
    state: String,
    message: String,
    setup_guidance: String,
}

impl WhisperModelPathTestView {
    fn invalid(message: impl Into<String>, setup_guidance: impl Into<String>) -> Self {
        Self {
            state: "Invalid".to_string(),
            message: message.into(),
            setup_guidance: setup_guidance.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaConnectionTestView {
    state: String,
    message: String,
    setup_guidance: String,
}

impl OllamaConnectionTestView {
    fn unavailable(message: impl Into<String>, setup_guidance: impl Into<String>) -> Self {
        Self {
            state: "Unavailable".to_string(),
            message: message.into(),
            setup_guidance: setup_guidance.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureStatus {
    microphone: DesktopPermissionState,
    system_audio: DesktopPermissionState,
}

#[derive(Clone, Copy, Debug, Serialize)]
enum DesktopPermissionState {
    Ready,
    MicrophoneDenied,
    MicrophoneUnavailable,
    SystemAudioDenied,
    SystemAudioUnavailable,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptionCommandView {
    meeting_id: String,
    state: TranscriptionCommandState,
    failure: Option<CommandFailureView>,
}

#[derive(Clone, Copy, Debug, Serialize)]
enum TranscriptionCommandState {
    Complete,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandFailureView {
    code: String,
    message: String,
    setup_guidance: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisCommandView {
    meeting_id: String,
    state: AnalysisCommandState,
    analysis: Option<AnalysisResultView>,
    failure: Option<CommandFailureView>,
}

impl AnalysisCommandView {
    fn from_command(command: AnalysisCommandDto) -> Self {
        Self {
            meeting_id: command.meeting_id,
            state: command.state,
            analysis: command.analysis.map(AnalysisResultView::from_analysis),
            failure: command.failure.map(|failure| CommandFailureView {
                code: failure.code,
                message: failure.message,
                setup_guidance: failure.setup_guidance,
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisResultView {
    provider: String,
    model_name: String,
    network_used: bool,
    summary: String,
}

impl AnalysisResultView {
    fn from_analysis(analysis: MeetingAnalysisDto) -> Self {
        Self {
            provider: analysis.provider,
            model_name: analysis.model_name,
            network_used: analysis.network_used,
            summary: analysis.summary,
        }
    }
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
#[serde(rename_all = "camelCase")]
struct ExportCommandState {
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meeting_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl Default for ExportCommandState {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            meeting_id: None,
            path: None,
            message: None,
        }
    }
}

impl ExportCommandState {
    fn exported(exported: ExportedMeetingDto) -> Self {
        Self {
            state: "exported".to_string(),
            meeting_id: Some(exported.meeting_id),
            path: Some(exported.path),
            message: None,
        }
    }

    fn failed(meeting_id: &str, message: String) -> Self {
        Self {
            state: "failed".to_string(),
            meeting_id: Some(meeting_id.to_string()),
            path: None,
            message: Some(message),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteCommandState {
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meeting_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    deleted_private_artifacts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped_private_artifacts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remaining_exports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl Default for DeleteCommandState {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            meeting_id: None,
            deleted_private_artifacts: Vec::new(),
            skipped_private_artifacts: Vec::new(),
            remaining_exports: Vec::new(),
            message: None,
        }
    }
}

impl DeleteCommandState {
    fn deleted(deleted: DeletedMeetingDto) -> Self {
        Self {
            state: "deleted".to_string(),
            meeting_id: Some(deleted.meeting_id),
            deleted_private_artifacts: deleted.deleted_private_artifacts,
            skipped_private_artifacts: deleted.skipped_private_artifacts,
            remaining_exports: deleted.remaining_exports,
            message: None,
        }
    }

    fn failed(meeting_id: &str, message: String) -> Self {
        Self {
            state: "failed".to_string(),
            meeting_id: Some(meeting_id.to_string()),
            deleted_private_artifacts: Vec::new(),
            skipped_private_artifacts: Vec::new(),
            remaining_exports: Vec::new(),
            message: Some(message),
        }
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
    summary: String,
    created_at_ms: u64,
    prompt_template_version: String,
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
    use curiosity_audio::{
        ArtifactManifest, AudioArtifactMetadata, CapturePermissionError, CaptureUnavailable,
        DeviceIdentity, ManifestStatus, RecordingMetadata, StreamKind,
    };
    use curiosity_domain::{
        AnalysisCitation, ArtifactKind, AudioArtifact, Meeting, MeetingAnalysis, ModelRun,
        RecordingSession, RecordingSource, SourceChannel, TranscriptSegment, TranscriptVersion,
    };
    use curiosity_transcription::{FakeWhisperBackend, WhisperBackendSegment};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn empty_desktop_snapshot_serializes_frontend_shape() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");
        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["loading"], false);
        assert_eq!(
            json["commandSurface"]["detail"],
            "Connected to local desktop commands."
        );
        assert_eq!(json["meetings"].as_array().expect("meetings").len(), 0);
        assert!(json["selectedMeetingId"].is_null());
        assert_eq!(json["recording"]["state"], "Idle");
        assert_eq!(json["recording"]["permission_state"], "Ready");
        assert_eq!(
            json["recording"]["recovery_action"],
            "Start a desktop recording to create private microphone and system audio WAV artifacts."
        );
        assert_eq!(
            json["recording"]["storage_location"]["app_private_path"],
            root.display().to_string()
        );
        assert_eq!(json["model"]["kind"], "missing");
        assert_eq!(json["capture"]["microphone"], "Ready");
        assert_eq!(json["capture"]["systemAudio"], "SystemAudioUnavailable");
        assert_eq!(json["settings"]["ollamaBaseUrl"], "http://127.0.0.1:11434");
        assert_eq!(json["settings"]["ollamaModel"], "qwen3.6:27b");
        assert!(json["transcription"].is_null());

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn get_settings_returns_default_local_analysis_settings() {
        let root = unique_test_root();

        let settings = get_settings_for_app_root(&root).expect("settings");

        assert_eq!(settings.whisper_model_path, "");
        assert_eq!(settings.ollama_base_url, "http://127.0.0.1:11434");
        assert_eq!(settings.ollama_model, "qwen3.6:27b");
        assert_eq!(settings.export_directory, None);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_settings_commands_persist_whisper_and_analysis_values() {
        let root = unique_test_root();

        save_whisper_model_path_for_app_root(&root, "/models/ggml-base.en.bin".to_string())
            .expect("save whisper");
        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11435".to_string(),
            "gemma4:31b".to_string(),
        )
        .expect("save analysis");
        let settings = get_settings_for_app_root(&root).expect("settings");

        assert_eq!(settings.whisper_model_path, "/models/ggml-base.en.bin");
        assert_eq!(settings.ollama_base_url, "http://127.0.0.1:11435");
        assert_eq!(settings.ollama_model, "gemma4:31b");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_ollama_client_posts_non_streaming_json_generate_request() {
        let transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Local summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[]}"}"#,
        );
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport.clone());

        let response = client
            .complete("qwen3.6:27b", "summarize locally")
            .expect("complete with fake local transport");

        assert!(response.contains("Local summary"));
        let request = transport.last_generate_request().expect("generate request");
        assert_eq!(request.url, "http://127.0.0.1:11434/api/generate");
        assert_eq!(request.body["model"], "qwen3.6:27b");
        assert_eq!(request.body["prompt"], "summarize locally");
        assert_eq!(request.body["stream"], false);
        assert_eq!(request.body["format"]["type"], "object");
        assert_eq!(
            request.body["format"]["properties"]["decisions"]["items"]["type"],
            "object"
        );
        assert_eq!(request.body["options"]["temperature"], 0);
    }

    #[test]
    fn local_ollama_client_rejects_non_loopback_base_url_before_transport_call() {
        let transport = RecordingOllamaTransport::generate_response(r#"{"response":"{}"}"#);
        let client = LocalOllamaTextClient::new("http://192.168.1.20:11434", transport.clone());

        let error = client
            .complete("qwen3.6:27b", "summarize locally")
            .expect_err("non-loopback Ollama should be rejected");

        assert!(error.to_string().contains("loopback"));
        assert_eq!(transport.generate_call_count(), 0);
    }

    #[test]
    fn local_ollama_client_rejects_cloud_model_tags_before_transport_call() {
        let transport = RecordingOllamaTransport::generate_response(r#"{"response":"{}"}"#);
        let client = LocalOllamaTextClient::new("http://localhost:11434", transport.clone());

        let error = client
            .complete("deepseek-v3.2:cloud", "summarize locally")
            .expect_err("cloud tags cannot use the local privacy path");

        assert!(error.to_string().contains("hosted"));
        assert_eq!(transport.generate_call_count(), 0);
    }

    #[test]
    fn test_ollama_connection_reports_reachable_configured_model_without_live_server() {
        let transport = RecordingOllamaTransport::tags_response(
            r#"{"models":[{"name":"qwen3.6:27b"},{"name":"gemma4:31b"}]}"#,
        );

        let result =
            test_ollama_connection_value("http://127.0.0.1:11434", "qwen3.6:27b", &transport);

        assert_eq!(result.state, "Available");
        assert!(result.message.contains("qwen3.6:27b"));
    }

    #[test]
    fn test_ollama_connection_accepts_model_field_without_name_field() {
        let transport =
            RecordingOllamaTransport::tags_response(r#"{"models":[{"model":"qwen3.6:27b"}]}"#);

        let result =
            test_ollama_connection_value("http://127.0.0.1:11434", "qwen3.6:27b", &transport);

        assert_eq!(result.state, "Available");
        assert!(result.message.contains("qwen3.6:27b"));
    }

    #[test]
    fn test_ollama_connection_accepts_tagless_request_when_latest_is_installed() {
        let transport =
            RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"gemma4:latest"}]}"#);

        let result = test_ollama_connection_value("http://127.0.0.1:11434", "gemma4", &transport);

        assert_eq!(result.state, "Available");
        assert!(result.message.contains("gemma4"));
    }

    #[test]
    fn test_ollama_connection_reports_missing_model_without_live_server() {
        let transport =
            RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"gemma4:31b"}]}"#);

        let result =
            test_ollama_connection_value("http://127.0.0.1:11434", "qwen3.6:27b", &transport);

        assert_eq!(result.state, "Unavailable");
        assert!(result.setup_guidance.contains("ollama pull qwen3.6:27b"));
        assert!(result
            .setup_guidance
            .contains("Installed local models: gemma4:31b"));
    }

    #[test]
    fn test_ollama_connection_includes_status_body_when_tags_returns_http_error() {
        let transport =
            RecordingOllamaTransport::tags_http_error(500, r#"{"error":"tags unavailable"}"#);

        let result =
            test_ollama_connection_value("http://127.0.0.1:11434", "qwen3.6:27b", &transport);

        assert_eq!(result.state, "Unavailable");
        assert!(result.message.contains("HTTP 500"));
        assert!(result.message.contains("tags unavailable"));
    }

    #[test]
    fn generate_summary_command_persists_local_ollama_summary_in_snapshot() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "We decided to keep summaries local.",
        );
        let transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Local Ollama summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[{\"segment_id\":\"meeting-1-segment-1\",\"start_ms\":0,\"end_ms\":1200}]}"}"#,
        );
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport);

        let snapshot = generate_summary_for_app_root_with_client(
            &root,
            &mut command_state,
            "meeting-1",
            client,
            "qwen3.6:27b",
            1_700_000_002_000,
        )
        .expect("generate summary snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["analysisCommand"]["state"], "Complete");
        assert_eq!(
            json["meetings"][0]["analysis"]["summary"],
            "Local Ollama summary"
        );
        assert_eq!(json["meetings"][0]["analysis"]["networkUsed"], false);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn generate_summary_command_shows_unavailable_ollama_without_persisting_analysis() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Unavailable Summary",
            "We need a visible setup failure.",
        );
        let transport = RecordingOllamaTransport::generate_error("connection refused");
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport);

        let snapshot = generate_summary_for_app_root_with_client(
            &root,
            &mut command_state,
            "meeting-1",
            client,
            "qwen3.6:27b",
            1_700_000_002_000,
        )
        .expect("failure snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["analysisCommand"]["state"], "Failed");
        assert_eq!(
            json["analysisCommand"]["failure"]["code"],
            "ollama_unavailable"
        );
        assert!(json["meetings"][0]["analysis"].is_null());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn generate_summary_command_preserves_ollama_http_status_body_as_transport_failure() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Missing Model Summary",
            "We need model-missing guidance from Ollama.",
        );
        let transport = RecordingOllamaTransport::generate_http_error(
            404,
            r#"{"error":"model 'qwen3.6:27b' not found"}"#,
        );
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport);

        let snapshot = generate_summary_for_app_root_with_client(
            &root,
            &mut command_state,
            "meeting-1",
            client,
            "qwen3.6:27b",
            1_700_000_002_000,
        )
        .expect("failure snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            json["analysisCommand"]["failure"]["code"],
            "provider_transport_error"
        );
        assert!(json["analysisCommand"]["failure"]["message"]
            .as_str()
            .expect("failure message")
            .contains("model 'qwen3.6:27b' not found"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn generate_summary_command_rejects_missing_transcript_before_ollama_call() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual("meeting-1", "No Transcript", 1_000))
            .expect("insert meeting");
        drop(store);
        let transport = RecordingOllamaTransport::generate_response(r#"{"response":"{}"}"#);
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport.clone());

        let snapshot = generate_summary_for_app_root_with_client(
            &root,
            &mut command_state,
            "meeting-1",
            client,
            "qwen3.6:27b",
            1_700_000_002_000,
        )
        .expect("missing transcript snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["analysisCommand"]["state"], "Failed");
        assert_eq!(
            json["analysisCommand"]["failure"]["code"],
            "no_transcript_segments"
        );
        assert_eq!(transport.generate_call_count(), 0);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn test_whisper_model_path_accepts_readable_file_without_loading_model() {
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"not a real model").expect("model file");

        let result = test_whisper_model_path_value(model_path.to_string_lossy().as_ref());

        assert_eq!(result.state, "Valid");
        assert!(result.message.contains("readable"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_whisper_model_path_rejects_missing_path_with_guidance() {
        let result = test_whisper_model_path_value("");

        assert_eq!(result.state, "Invalid");
        assert!(result.setup_guidance.contains("local Whisper model path"));
    }

    #[test]
    fn desktop_snapshot_uses_env_whisper_path_when_settings_path_is_empty() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let model_path = root.join("env-whisper.bin");
        fs::write(&model_path, b"fixture").expect("model file");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::set_var("CURIOSITY_WHISPER_MODEL", &model_path);

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["model"]["kind"], "ready");
        assert_eq!(
            json["model"]["configuredPath"],
            model_path.to_string_lossy().as_ref()
        );

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_snapshot_prefers_persisted_whisper_path_over_env_fallback() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let env_model_path = root.join("env-whisper.bin");
        let saved_model_path = root.join("saved-whisper.bin");
        fs::write(&env_model_path, b"fixture").expect("env model file");
        fs::write(&saved_model_path, b"fixture").expect("saved model file");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::set_var("CURIOSITY_WHISPER_MODEL", &env_model_path);
        save_whisper_model_path_for_app_root(&root, saved_model_path.to_string_lossy().to_string())
            .expect("save whisper");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            json["model"]["configuredPath"],
            saved_model_path.to_string_lossy().as_ref()
        );

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(not(feature = "system-audio-screencapturekit"))]
    fn system_audio_smoke_recording_reports_unavailable_without_fake_success() {
        let root = unique_test_root();

        let status =
            system_audio_smoke_recording_for_app_root(&root, std::time::Duration::from_millis(1));

        assert_ne!(status.state, "Passed");
        assert!(
            status.message.contains("ScreenCaptureKit")
                || status.message.contains("Screen Recording")
        );

        let _ = fs::remove_dir_all(root);
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
        assert_eq!(
            meeting["privacy"]["storagePath"],
            "meetings/meeting-1/audio"
        );
        assert_eq!(meeting["analysis"]["modelName"], "qwen3:30b");
        assert_eq!(meeting["analysis"]["networkUsed"], false);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn search_meetings_command_returns_fts_backed_result_ids_and_titles() {
        let root = unique_test_root();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Planning Alpha",
            "launch readiness",
        );
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-2",
            "Design Review",
            "layout density",
        );

        let results = search_meetings_for_app_root(&root, "launch").expect("search meetings");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].meeting_id, "meeting-1");
        assert_eq!(results[0].title, "Planning Alpha");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn seed_dev_fixture_for_app_root_returns_transcript_ready_snapshot_without_duplication() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");
        let command_state = DesktopCommandState::default();

        let first_snapshot =
            seed_dev_fixture_for_app_root(&root, &command_state).expect("seed fixture");
        let second_snapshot =
            seed_dev_fixture_for_app_root(&root, &command_state).expect("seed fixture again");
        let first = serde_json::to_value(&first_snapshot).expect("serialize first snapshot");
        let second = serde_json::to_value(&second_snapshot).expect("serialize second snapshot");
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription("dev-fixture-meeting")
            .expect("query completed artifact")
            .expect("completed fixture artifact");

        assert_eq!(first["meetings"][0]["id"], "dev-fixture-meeting");
        assert_eq!(first["meetings"][0]["title"], "Dev Fixture Full Cycle");
        assert_eq!(first["meetings"][0]["transcriptState"], "Ready");
        assert_eq!(
            first["meetings"][0]["segments"]
                .as_array()
                .expect("segments")
                .len(),
            2
        );
        assert_eq!(second, first);
        assert_eq!(store.count("meetings").expect("meetings"), 1);
        assert_eq!(store.count("recording_sessions").expect("sessions"), 1);
        assert_eq!(store.count("audio_artifacts").expect("artifacts"), 1);
        assert_eq!(store.count("model_runs").expect("model runs"), 1);
        assert_eq!(store.count("transcript_versions").expect("versions"), 1);
        assert_eq!(store.count("transcript_segments").expect("segments"), 2);
        assert_eq!(artifact.artifact_id, "dev-fixture-artifact");
        assert_eq!(
            format!("{:x}", Sha256::digest(dev_fixture_wav_bytes())),
            DEV_FIXTURE_AUDIO_SHA256
        );
        assert_eq!(artifact.sha256, DEV_FIXTURE_AUDIO_SHA256);
        assert!(root.join(&artifact.path).is_file());

        restore_whisper_env(previous);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn seed_dev_fixture_for_app_root_fails_loudly_when_fixed_id_is_partial_fixture() {
        let root = unique_test_root();
        let command_state = DesktopCommandState::default();
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual(
                "dev-fixture-meeting",
                "Dev Fixture Full Cycle",
                1_700_000_000_000,
            ))
            .expect("insert partial fixture");

        let error = seed_dev_fixture_for_app_root(&root, &command_state)
            .expect_err("partial fixture should fail loudly");

        assert!(error.contains("partial dev fixture"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn seed_dev_fixture_for_app_root_fails_loudly_when_private_audio_file_is_missing() {
        let root = unique_test_root();
        let command_state = DesktopCommandState::default();
        seed_dev_fixture_for_app_root(&root, &command_state).expect("seed fixture");
        fs::remove_file(root.join(DEV_FIXTURE_ARTIFACT_PATH)).expect("remove fixture audio");

        let error = seed_dev_fixture_for_app_root(&root, &command_state)
            .expect_err("missing fixture audio should fail loudly");

        assert!(error.contains("private audio artifact file is missing"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn seed_dev_fixture_for_app_root_supports_search_export_and_delete_workflow() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_dev_fixture_for_app_root(&root, &command_state).expect("seed fixture");

        let title_results =
            search_meetings_for_app_root(&root, "Fixture").expect("search by title");
        let transcript_results =
            search_meetings_for_app_root(&root, "deterministic").expect("search by transcript");
        let export_snapshot =
            export_meeting_json_for_app_root(&root, &mut command_state, "dev-fixture-meeting")
                .expect("export fixture");
        let export_json = serde_json::to_value(&export_snapshot).expect("serialize export");
        let exported_path = export_json["exportCommand"]["path"]
            .as_str()
            .expect("export path");
        let export = Store::read_meeting_export_json(exported_path).expect("read export");
        let private_path = root.join("meetings/dev-fixture-meeting/audio/imported.wav");
        let delete_snapshot =
            delete_meeting_for_app_root(&root, &mut command_state, "dev-fixture-meeting")
                .expect("delete fixture");
        let delete_json = serde_json::to_value(&delete_snapshot).expect("serialize delete");

        assert_eq!(title_results[0].meeting_id, "dev-fixture-meeting");
        assert_eq!(transcript_results[0].meeting_id, "dev-fixture-meeting");
        assert_eq!(export.meeting_id, "dev-fixture-meeting");
        assert_eq!(export.segments.len(), 2);
        assert!(!private_path.exists());
        assert_eq!(
            delete_json["meetings"].as_array().expect("meetings").len(),
            0
        );
        assert_eq!(delete_json["deleteCommand"]["state"], "deleted");
        assert!(PathBuf::from(exported_path).exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn seed_dev_fixture_for_app_root_supports_summary_with_injected_ollama_client() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_dev_fixture_for_app_root(&root, &command_state).expect("seed fixture");
        let transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Fixture summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[{\"segment_id\":\"dev-fixture-segment-1\",\"start_ms\":0,\"end_ms\":1500}]}"}"#,
        );
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport);

        let snapshot = generate_summary_for_app_root_with_client(
            &root,
            &mut command_state,
            "dev-fixture-meeting",
            client,
            "qwen3.6:27b",
            1_700_000_002_000,
        )
        .expect("generate fixture summary");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["analysisCommand"]["state"], "Complete");
        assert_eq!(
            json["analysisCommand"]["analysis"]["summary"],
            "Fixture summary"
        );
        assert_eq!(
            json["meetings"][0]["analysis"]["summary"],
            "Fixture summary"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_meeting_command_updates_selected_snapshot_title() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Original Planning",
            "rename target",
        );

        let snapshot =
            rename_meeting_for_app_root(&root, &mut command_state, "meeting-1", "Renamed Planning")
                .expect("rename meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["selectedMeetingId"], "meeting-1");
        assert_eq!(json["meetings"][0]["title"], "Renamed Planning");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn export_meeting_json_command_writes_json_and_exposes_export_state() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Export Planning",
            "export this transcript",
        );

        let snapshot = export_meeting_json_for_app_root(&root, &mut command_state, "meeting-1")
            .expect("export meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let exported_path = json["exportCommand"]["path"].as_str().expect("export path");
        let export = Store::read_meeting_export_json(exported_path).expect("read export");

        assert_eq!(json["exportCommand"]["state"], "exported");
        assert_eq!(json["meetings"][0]["exportState"]["path"], exported_path);
        assert_eq!(export.meeting_id, "meeting-1");
        assert_eq!(export.segments[0].text, "export this transcript");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn delete_meeting_command_removes_private_data_and_preserves_visible_delete_outcome() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Delete Planning",
            "delete this transcript",
        );
        let export_snapshot =
            export_meeting_json_for_app_root(&root, &mut command_state, "meeting-1")
                .expect("export meeting");
        let export_json = serde_json::to_value(&export_snapshot).expect("serialize export");
        let exported_path = export_json["exportCommand"]["path"]
            .as_str()
            .expect("export path")
            .to_string();
        let private_path = root.join("meetings/meeting-1/audio/imported.wav");

        let snapshot = delete_meeting_for_app_root(&root, &mut command_state, "meeting-1")
            .expect("delete meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let meetings = json["meetings"].as_array().expect("meetings");

        assert!(!private_path.exists());
        assert!(meetings.iter().all(|meeting| meeting["id"] != "meeting-1"));
        assert_eq!(json["deleteCommand"]["state"], "deleted");
        assert_eq!(json["deleteCommand"]["remainingExports"][0], exported_path);
        assert!(PathBuf::from(exported_path).exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn start_microphone_recording_with_fake_recorder_returns_active_snapshot() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMicrophoneRecorderFactory;

        let snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("MVP check".to_string()),
            1_700_000_000_000,
            &factory,
        )
        .expect("start recording");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["recording"]["state"], "Recording");
        assert_eq!(json["recording"]["permission_state"], "Ready");
        assert_eq!(json["meetings"][0]["title"], "MVP check");
        assert_eq!(json["meetings"][0]["status"], "Recording");
        assert_eq!(json["selectedMeetingId"], json["recording"]["meeting_id"]);
        assert!(command_state.active_recording.is_some());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn stop_microphone_recording_persists_complete_private_wav_artifact() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Recorded locally".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let snapshot =
            stop_microphone_recording_for_app_root(&root, &mut command_state, started_at_ms + 500)
                .expect("stop recording");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query artifact")
            .expect("complete artifact");

        assert!(command_state.active_recording.is_none());
        assert_eq!(json["recording"]["state"], "Complete");
        assert_eq!(json["capture"]["microphone"], "Ready");
        assert_eq!(json["meetings"][0]["status"], "Complete");
        assert_eq!(artifact.sha256.len(), 64);
        assert!(root.join(&artifact.path).is_file());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn stop_recording_persists_complete_microphone_and_system_wav_artifacts() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMixedRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Full call".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start mixed recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        stop_microphone_recording_for_app_root(&root, &mut command_state, started_at_ms + 500)
            .expect("stop mixed recording");
        let store = open_store(&root).expect("open store");
        let artifact_root = root.join("meetings").join(meeting_id).join("audio");

        assert!(command_state.active_recording.is_none());
        assert_eq!(store.count("audio_artifacts").expect("artifacts"), 2);
        assert!(artifact_root
            .join("recording-1700000000000/raw-mic.wav")
            .is_file());
        assert!(artifact_root
            .join("recording-1700000000000/raw-system.wav")
            .is_file());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn startup_repair_recovers_crashed_microphone_wav_for_transcription() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Crash recovery".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let recording_id = start_snapshot
            .recording
            .recording_id
            .clone()
            .expect("recording id");
        let artifact_path = root.join(microphone_artifact_relative_path(
            &meeting_id,
            &recording_id,
        ));
        fs::create_dir_all(artifact_path.parent().expect("artifact parent")).expect("artifact dir");
        write_minimal_wav(&artifact_path);

        let restarted_store = open_store(&root).expect("open repaired store");
        let artifact = restarted_store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query completed artifact")
            .expect("recovered artifact");

        assert_eq!(
            artifact.path,
            microphone_artifact_relative_path(&meeting_id, &recording_id)
        );
        assert_eq!(artifact.sha256.len(), 64);
        assert!(!artifact.sha256.starts_with("sha256:pending"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn startup_repair_recovers_crashed_mixed_wavs_for_transcription() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMixedRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Mixed crash recovery".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let recording_id = start_snapshot
            .recording
            .recording_id
            .clone()
            .expect("recording id");
        let mic_path = root.join(microphone_artifact_relative_path(
            &meeting_id,
            &recording_id,
        ));
        let system_path = root.join(system_audio_artifact_relative_path(
            &meeting_id,
            &recording_id,
        ));
        fs::create_dir_all(mic_path.parent().expect("artifact parent")).expect("artifact dir");
        write_minimal_wav(&mic_path);
        write_minimal_wav(&system_path);

        let restarted_store = open_store(&root).expect("open repaired store");
        let artifacts = restarted_store
            .completed_wav_artifacts_for_transcription(&meeting_id)
            .expect("query completed artifacts");

        assert_eq!(
            artifacts
                .iter()
                .map(|artifact| artifact.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["RawMic", "RawSystem"]
        );
        assert!(artifacts
            .iter()
            .all(|artifact| !artifact.sha256.starts_with("sha256:pending")));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn stop_failure_marks_recording_failed_without_stale_active_state() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FailingStopMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Failing stop".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let snapshot =
            stop_microphone_recording_for_app_root(&root, &mut command_state, started_at_ms + 500)
                .expect("stop failure snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");

        assert!(command_state.active_recording.is_none());
        assert_eq!(json["recording"]["state"], "Interrupted");
        assert_eq!(
            json["recording"]["permission_state"],
            "MicrophoneUnavailable"
        );
        assert_eq!(
            store.meeting_status(&meeting_id).expect("meeting status"),
            "Failed"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn stop_missing_system_artifact_completes_transcribable_mic_only_recording() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = PartialMixedRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Partial mixed".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start partial mixed recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let snapshot =
            stop_microphone_recording_for_app_root(&root, &mut command_state, started_at_ms + 500)
                .expect("stop partial mixed recording");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");

        assert_eq!(json["recording"]["state"], "Complete");
        assert_eq!(json["recording"]["permission_state"], "Ready");
        assert_eq!(
            store.meeting_status(&meeting_id).expect("meeting status"),
            "Complete"
        );
        let artifacts = store
            .completed_wav_artifacts_for_transcription(&meeting_id)
            .expect("completed artifacts")
            .into_iter()
            .map(|artifact| artifact.kind)
            .collect::<Vec<_>>();
        assert_eq!(artifacts, vec!["RawMic"]);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn system_audio_startup_errors_allow_microphone_fallback() {
        let system_unavailable = CaptureError::Unavailable(CaptureUnavailable::system_audio(
            "ScreenCaptureKit adapter unavailable",
        ));
        let system_permission = CaptureError::PermissionDenied(CapturePermissionError::denied(
            CapturePermission::SystemAudioScreenRecording,
        ));
        let microphone_unavailable =
            CaptureError::Unavailable(CaptureUnavailable::microphone("no input device"));
        let microphone_permission = CaptureError::PermissionDenied(CapturePermissionError::denied(
            CapturePermission::Microphone,
        ));

        assert!(can_fallback_to_microphone_recording(&system_unavailable));
        assert!(can_fallback_to_microphone_recording(&system_permission));
        assert!(!can_fallback_to_microphone_recording(
            &microphone_unavailable
        ));
        assert!(!can_fallback_to_microphone_recording(
            &microphone_permission
        ));
    }

    #[test]
    fn start_persistence_failure_stops_started_recorder_without_partial_rows() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let started_at_ms = 1_700_000_000_000;
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual(
                format!("meeting-{started_at_ms}"),
                "Existing",
                started_at_ms,
            ))
            .expect("seed duplicate meeting");
        let cleanup_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let factory = CleanupTrackingMicrophoneRecorderFactory {
            cleanup_count: std::sync::Arc::clone(&cleanup_count),
        };

        let error = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Duplicate".to_string()),
            started_at_ms,
            &factory,
        )
        .expect_err("duplicate meeting should fail after recorder start");

        assert!(error
            .message
            .contains("Recording metadata could not be persisted"));
        assert_eq!(cleanup_count.load(Ordering::SeqCst), 1);
        assert!(command_state.active_recording.is_none());
        assert_eq!(store.count("recording_sessions").expect("sessions"), 0);
        assert_eq!(store.count("audio_artifacts").expect("artifacts"), 0);
        assert!(!root
            .join(format!(
                "meetings/meeting-{started_at_ms}/audio/recording-{started_at_ms}"
            ))
            .exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn start_permission_denied_failure_preserves_macos_privacy_guidance() {
        let root = unique_test_root();
        let error = MicrophoneStartFailure::permission_denied("Microphone permission is denied");
        let dto = start_failure_recording_dto(&root, &error);

        assert_eq!(dto.permission_state, AppPermissionState::MicrophoneDenied);
        assert!(dto.recovery_action.contains("Privacy & Security"));
        assert!(dto.recovery_action.contains("Microphone"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transcribe_missing_model_returns_visible_failure_snapshot() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let meeting_id = seed_stopped_fake_recording(&root, &mut command_state);

        let snapshot = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            root.join("missing-model.bin"),
            "missing-model.bin",
            FakeWhisperBackend::default(),
            1_700_000_001_000,
        )
        .expect("transcription failure is represented in snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["transcription"]["state"], "Failed");
        assert_eq!(json["transcription"]["failure"]["code"], "missing_model");
        assert!(json["transcription"]["failure"]["message"]
            .as_str()
            .expect("failure message")
            .contains("CURIOSITY_WHISPER_MODEL"));
        assert_eq!(json["meetings"][0]["transcriptState"], "Unavailable");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcribe_with_fake_backend_persists_segments_and_returns_ready_snapshot() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let meeting_id = seed_stopped_fake_recording(&root, &mut command_state);
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");
        let backend = FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
            0,
            1_200,
            "local transcript",
        )]);

        let snapshot = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            backend,
            1_700_000_001_000,
        )
        .expect("transcribe meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["transcription"]["state"], "Complete");
        assert_eq!(json["meetings"][0]["transcriptState"], "Ready");
        assert_eq!(json["meetings"][0]["transcriptText"], "local transcript");
        assert_eq!(
            json["meetings"][0]["segments"][0]["sourceChannel"],
            "Microphone"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcribe_mixed_recording_persists_microphone_and_system_segments() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMixedRecorderFactory;
        let started_at_ms = 1_700_000_000_000;
        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Full transcript".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start mixed recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        stop_microphone_recording_for_app_root(&root, &mut command_state, started_at_ms + 500)
            .expect("stop mixed recording");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");

        let snapshot = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            PathAwareWhisperBackend,
            1_700_000_001_000,
        )
        .expect("transcribe mixed meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let segments = json["meetings"][0]["segments"]
            .as_array()
            .expect("segments");

        assert_eq!(json["transcription"]["state"], "Complete");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0]["sourceChannel"], "Microphone");
        assert_eq!(segments[0]["text"], "mic side");
        assert_eq!(segments[1]["sourceChannel"], "System");
        assert_eq!(segments[1]["text"], "call side");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcribe_persistence_conflict_replaces_stale_success_with_visible_failure() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let meeting_id = seed_stopped_fake_recording(&root, &mut command_state);
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");

        transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path.clone(),
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(0, 1_200, "first")]),
            1_700_000_001_000,
        )
        .expect("first transcript");
        let snapshot = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(0, 1_200, "changed")]),
            1_700_000_002_000,
        )
        .expect("persist conflict is represented in snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["transcription"]["state"], "Failed");
        assert_eq!(
            json["transcription"]["failure"]["code"],
            "transcript_persist_failed"
        );
        assert_eq!(json["meetings"][0]["transcriptText"], "first");

        fs::remove_dir_all(root).expect("cleanup");
    }

    fn seed_stopped_fake_recording(root: &Path, command_state: &mut DesktopCommandState) -> String {
        let factory = FakeMicrophoneRecorderFactory;
        let snapshot = start_microphone_recording_for_app_root(
            root,
            command_state,
            Some("Ready to transcribe".to_string()),
            1_700_000_000_000,
            &factory,
        )
        .expect("start fake recording");
        let meeting_id = snapshot.recording.meeting_id.clone();
        stop_microphone_recording_for_app_root(root, command_state, 1_700_000_000_500)
            .expect("stop fake recording");
        meeting_id
    }

    #[derive(Default)]
    struct FakeMicrophoneRecorderFactory;

    impl MicrophoneRecorderFactory for FakeMicrophoneRecorderFactory {
        fn start(
            &self,
            audio_root: &Path,
            recording_id: &str,
            started_at_ms: u64,
        ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
            Ok(StartedMicrophoneRecording {
                sample_rate_hz: 48_000,
                streams: vec![StreamKind::Microphone],
                recorder: Box::new(FakeActiveMicrophoneRecording {
                    session_dir: audio_root.join(recording_id),
                    recording_id: recording_id.to_string(),
                    started_at_ms,
                }),
            })
        }
    }

    struct FakeActiveMicrophoneRecording {
        session_dir: PathBuf,
        recording_id: String,
        started_at_ms: u64,
    }

    struct FakeMixedRecorderFactory;

    impl MicrophoneRecorderFactory for FakeMixedRecorderFactory {
        fn start(
            &self,
            audio_root: &Path,
            recording_id: &str,
            started_at_ms: u64,
        ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
            Ok(StartedMicrophoneRecording {
                sample_rate_hz: 48_000,
                streams: vec![StreamKind::Microphone, StreamKind::SystemAudio],
                recorder: Box::new(FakeActiveMixedRecording {
                    session_dir: audio_root.join(recording_id),
                    recording_id: recording_id.to_string(),
                    started_at_ms,
                }),
            })
        }
    }

    struct FakeActiveMixedRecording {
        session_dir: PathBuf,
        recording_id: String,
        started_at_ms: u64,
    }

    struct PartialMixedRecorderFactory;

    impl MicrophoneRecorderFactory for PartialMixedRecorderFactory {
        fn start(
            &self,
            audio_root: &Path,
            recording_id: &str,
            started_at_ms: u64,
        ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
            Ok(StartedMicrophoneRecording {
                sample_rate_hz: 48_000,
                streams: vec![StreamKind::Microphone, StreamKind::SystemAudio],
                recorder: Box::new(FakeActivePartialMixedRecording {
                    session_dir: audio_root.join(recording_id),
                    recording_id: recording_id.to_string(),
                    started_at_ms,
                }),
            })
        }
    }

    struct FakeActivePartialMixedRecording {
        session_dir: PathBuf,
        recording_id: String,
        started_at_ms: u64,
    }

    struct FailingStopMicrophoneRecorderFactory;

    impl MicrophoneRecorderFactory for FailingStopMicrophoneRecorderFactory {
        fn start(
            &self,
            _audio_root: &Path,
            _recording_id: &str,
            _started_at_ms: u64,
        ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
            Ok(StartedMicrophoneRecording {
                sample_rate_hz: 48_000,
                streams: vec![StreamKind::Microphone],
                recorder: Box::new(FailingStopMicrophoneRecording),
            })
        }
    }

    struct CleanupTrackingMicrophoneRecorderFactory {
        cleanup_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl MicrophoneRecorderFactory for CleanupTrackingMicrophoneRecorderFactory {
        fn start(
            &self,
            audio_root: &Path,
            recording_id: &str,
            started_at_ms: u64,
        ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
            Ok(StartedMicrophoneRecording {
                sample_rate_hz: 48_000,
                streams: vec![StreamKind::Microphone],
                recorder: Box::new(CleanupTrackingMicrophoneRecording {
                    session_dir: audio_root.join(recording_id),
                    recording_id: recording_id.to_string(),
                    started_at_ms,
                    cleanup_count: std::sync::Arc::clone(&self.cleanup_count),
                }),
            })
        }
    }

    struct CleanupTrackingMicrophoneRecording {
        session_dir: PathBuf,
        recording_id: String,
        started_at_ms: u64,
        cleanup_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ActiveMicrophoneRecording for CleanupTrackingMicrophoneRecording {
        fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
            self.cleanup_count.fetch_add(1, Ordering::SeqCst);
            Box::new(FakeActiveMicrophoneRecording {
                session_dir: self.session_dir,
                recording_id: self.recording_id,
                started_at_ms: self.started_at_ms,
            })
            .stop(ended_at_ms)
        }
    }

    struct FailingStopMicrophoneRecording;

    impl ActiveMicrophoneRecording for FailingStopMicrophoneRecording {
        fn stop(self: Box<Self>, _ended_at_ms: u64) -> Result<ArtifactManifest, String> {
            Err("microphone stream produced no samples".to_string())
        }
    }

    impl ActiveMicrophoneRecording for FakeActiveMixedRecording {
        fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
            fs::create_dir_all(&self.session_dir).map_err(|error| error.to_string())?;
            let mic_path = self.session_dir.join("raw-mic.wav");
            let system_path = self.session_dir.join("raw-system.wav");
            write_minimal_wav(&mic_path);
            write_minimal_wav(&system_path);
            Ok(ArtifactManifest {
                recording: RecordingMetadata::new(&self.recording_id, self.started_at_ms),
                status: ManifestStatus::Complete,
                ended_at_ms: Some(ended_at_ms),
                artifacts: vec![
                    AudioArtifactMetadata {
                        stream: StreamKind::Microphone,
                        file_name: "raw-mic.wav".to_string(),
                        path: mic_path,
                        started_at_ms: self.started_at_ms,
                        ended_at_ms: Some(ended_at_ms),
                        duration_ms: ended_at_ms.saturating_sub(self.started_at_ms),
                        sample_rate_hz: 48_000,
                        channel_count: 1,
                        identity: DeviceIdentity::new("fake-mic", "Fake Microphone", "test"),
                        bytes_written: 44,
                        sha256: "d0c7ca55e6fde29961f3cebe41e0ee7f532f2040c3a5689e62d1fd168ea267a1"
                            .to_string(),
                    },
                    AudioArtifactMetadata {
                        stream: StreamKind::SystemAudio,
                        file_name: "raw-system.wav".to_string(),
                        path: system_path,
                        started_at_ms: self.started_at_ms,
                        ended_at_ms: Some(ended_at_ms),
                        duration_ms: ended_at_ms.saturating_sub(self.started_at_ms),
                        sample_rate_hz: 48_000,
                        channel_count: 2,
                        identity: DeviceIdentity::new("fake-system", "Fake System Audio", "test"),
                        bytes_written: 44,
                        sha256: "8cf248ff65c6a51c4ab1d46fd56f36d6235ea8318d07da5b84d58d3e647e8825"
                            .to_string(),
                    },
                ],
                recovery: None,
            })
        }
    }

    impl ActiveMicrophoneRecording for FakeActivePartialMixedRecording {
        fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
            fs::create_dir_all(&self.session_dir).map_err(|error| error.to_string())?;
            let mic_path = self.session_dir.join("raw-mic.wav");
            write_minimal_wav(&mic_path);
            Ok(ArtifactManifest {
                recording: RecordingMetadata::new(&self.recording_id, self.started_at_ms),
                status: ManifestStatus::Complete,
                ended_at_ms: Some(ended_at_ms),
                artifacts: vec![AudioArtifactMetadata {
                    stream: StreamKind::Microphone,
                    file_name: "raw-mic.wav".to_string(),
                    path: mic_path,
                    started_at_ms: self.started_at_ms,
                    ended_at_ms: Some(ended_at_ms),
                    duration_ms: ended_at_ms.saturating_sub(self.started_at_ms),
                    sample_rate_hz: 48_000,
                    channel_count: 1,
                    identity: DeviceIdentity::new("fake-mic", "Fake Microphone", "test"),
                    bytes_written: 44,
                    sha256: "d0c7ca55e6fde29961f3cebe41e0ee7f532f2040c3a5689e62d1fd168ea267a1"
                        .to_string(),
                }],
                recovery: None,
            })
        }
    }

    impl ActiveMicrophoneRecording for FakeActiveMicrophoneRecording {
        fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
            fs::create_dir_all(&self.session_dir).map_err(|error| error.to_string())?;
            let path = self.session_dir.join("raw-mic.wav");
            write_minimal_wav(&path);
            Ok(ArtifactManifest {
                recording: RecordingMetadata::new(&self.recording_id, self.started_at_ms),
                status: ManifestStatus::Complete,
                ended_at_ms: Some(ended_at_ms),
                artifacts: vec![AudioArtifactMetadata {
                    stream: StreamKind::Microphone,
                    file_name: "raw-mic.wav".to_string(),
                    path,
                    started_at_ms: self.started_at_ms,
                    ended_at_ms: Some(ended_at_ms),
                    duration_ms: ended_at_ms.saturating_sub(self.started_at_ms),
                    sample_rate_hz: 48_000,
                    channel_count: 1,
                    identity: DeviceIdentity::new("fake-mic", "Fake Microphone", "test"),
                    bytes_written: 44,
                    sha256: "d0c7ca55e6fde29961f3cebe41e0ee7f532f2040c3a5689e62d1fd168ea267a1"
                        .to_string(),
                }],
                recovery: None,
            })
        }
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct PathAwareWhisperBackend;

    impl WhisperBackend for PathAwareWhisperBackend {
        fn provider(&self) -> &'static str {
            "local-whisper"
        }

        fn transcribe(
            &self,
            _model_path: &Path,
            audio_path: &Path,
        ) -> Result<Vec<WhisperBackendSegment>, TranscriptionError> {
            let file_name = audio_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let text = match file_name {
                "raw-system.wav" => "call side",
                _ => "mic side",
            };
            Ok(vec![WhisperBackendSegment::new(0, 1_200, text)])
        }
    }

    fn write_minimal_wav(path: &Path) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&38u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16_000u32.to_le_bytes());
        bytes.extend_from_slice(&32_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes());
        fs::write(path, bytes).expect("minimal wav");
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

    fn seed_transcribed_meeting_with_private_artifact(
        root: &Path,
        meeting_id: &str,
        title: &str,
        text: &str,
    ) {
        let store = open_store(root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual(meeting_id, title, 1_000))
            .expect("insert meeting");
        let session_id = format!("{meeting_id}-session-1");
        let session = RecordingSession::start(
            &session_id,
            meeting_id,
            RecordingSource::Imported,
            1_000,
            48_000,
        );
        store
            .insert_recording_session(&session)
            .expect("insert session");
        let artifact_path = format!("meetings/{meeting_id}/audio/imported.wav");
        let absolute_artifact_path = root.join(&artifact_path);
        fs::create_dir_all(
            absolute_artifact_path
                .parent()
                .expect("private artifact parent"),
        )
        .expect("private artifact dir");
        fs::write(&absolute_artifact_path, b"private audio").expect("private artifact");
        store
            .insert_audio_artifact(&AudioArtifact::new_private(
                format!("{meeting_id}-artifact-1"),
                &session_id,
                ArtifactKind::Imported,
                artifact_path,
                format!("sha256:{meeting_id}"),
            ))
            .expect("insert artifact");
        let run = ModelRun::new(
            format!("{meeting_id}-run-1"),
            meeting_id,
            format!("sha256:{meeting_id}"),
            "fake-local",
            "fixture-whisper",
            false,
            2_000,
        );
        let version = TranscriptVersion::new(
            format!("{meeting_id}-version-1"),
            meeting_id,
            format!("{meeting_id}-run-1"),
            1,
            2_010,
        );
        store
            .persist_transcript(
                &run,
                &version,
                &[TranscriptSegment::with_metadata(
                    format!("{meeting_id}-segment-1"),
                    meeting_id,
                    0,
                    1_200,
                    text,
                    SourceChannel::Imported,
                    &run.id,
                    &version.id,
                )],
            )
            .expect("persist transcript");
    }

    #[derive(Clone)]
    struct RecordingOllamaTransport {
        state: std::sync::Arc<Mutex<RecordingOllamaTransportState>>,
    }

    #[derive(Default)]
    struct RecordingOllamaTransportState {
        generate_response: Option<Result<serde_json::Value, OllamaHttpError>>,
        tags_response: Option<Result<serde_json::Value, OllamaHttpError>>,
        generate_requests: Vec<RecordedOllamaRequest>,
    }

    #[derive(Clone)]
    struct RecordedOllamaRequest {
        url: String,
        body: serde_json::Value,
    }

    impl RecordingOllamaTransport {
        fn generate_response(json: &str) -> Self {
            Self::new_with_generate(
                serde_json::from_str(json)
                    .map_err(|error| OllamaHttpError::MalformedResponse(error.to_string())),
            )
        }

        fn generate_error(error: &str) -> Self {
            Self::new_with_generate(Err(OllamaHttpError::Unavailable(error.to_string())))
        }

        fn generate_http_error(status: u16, body: &str) -> Self {
            Self::new_with_generate(Err(OllamaHttpError::Http {
                status,
                body: body.to_string(),
            }))
        }

        fn tags_response(json: &str) -> Self {
            let state = RecordingOllamaTransportState {
                tags_response: Some(
                    serde_json::from_str(json)
                        .map_err(|error| OllamaHttpError::MalformedResponse(error.to_string())),
                ),
                ..RecordingOllamaTransportState::default()
            };
            Self {
                state: std::sync::Arc::new(Mutex::new(state)),
            }
        }

        fn tags_http_error(status: u16, body: &str) -> Self {
            let state = RecordingOllamaTransportState {
                tags_response: Some(Err(OllamaHttpError::Http {
                    status,
                    body: body.to_string(),
                })),
                ..RecordingOllamaTransportState::default()
            };
            Self {
                state: std::sync::Arc::new(Mutex::new(state)),
            }
        }

        fn new_with_generate(response: Result<serde_json::Value, OllamaHttpError>) -> Self {
            let state = RecordingOllamaTransportState {
                generate_response: Some(response),
                ..RecordingOllamaTransportState::default()
            };
            Self {
                state: std::sync::Arc::new(Mutex::new(state)),
            }
        }

        fn last_generate_request(&self) -> Option<RecordedOllamaRequest> {
            self.state
                .lock()
                .expect("transport state")
                .generate_requests
                .last()
                .cloned()
        }

        fn generate_call_count(&self) -> usize {
            self.state
                .lock()
                .expect("transport state")
                .generate_requests
                .len()
        }
    }

    impl OllamaHttpTransport for RecordingOllamaTransport {
        fn post_json(
            &self,
            url: &str,
            body: serde_json::Value,
        ) -> Result<serde_json::Value, OllamaHttpError> {
            let mut state = self.state.lock().expect("transport state");
            state.generate_requests.push(RecordedOllamaRequest {
                url: url.to_string(),
                body,
            });
            state.generate_response.clone().unwrap_or_else(|| {
                Err(OllamaHttpError::Unavailable(
                    "missing generate response".to_string(),
                ))
            })
        }

        fn get_json(&self, _url: &str) -> Result<serde_json::Value, OllamaHttpError> {
            self.state
                .lock()
                .expect("transport state")
                .tags_response
                .clone()
                .unwrap_or_else(|| {
                    Err(OllamaHttpError::Unavailable(
                        "missing tags response".to_string(),
                    ))
                })
        }
    }

    fn unique_test_root() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let suffix = TEST_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("curiosity-desktop-command-test-{nanos}-{suffix}"))
    }

    fn restore_whisper_env(previous: Option<String>) {
        if let Some(previous) = previous {
            std::env::set_var("CURIOSITY_WHISPER_MODEL", previous);
        } else {
            std::env::remove_var("CURIOSITY_WHISPER_MODEL");
        }
    }
}
