use std::io::{Read, Seek, SeekFrom};
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use curiosity_analysis::{
    recommended_analysis_model_presets, summary_json_schema, AnalysisClientError,
    AnalysisProviderKind, OllamaAnalyzer, ProviderTextClient,
};
use curiosity_app::{
    attach_calendar_event_context_command, correct_transcript_segment_command,
    delete_meeting_command, export_meeting_command, export_meeting_json_command,
    generate_summary_command_with_cancellation, list_meetings_dto, meeting_detail_dto,
    rename_meeting_command, search_meetings_dto, AnalysisCommandDto, AnalysisCommandState,
    AppPermissionState, CalendarEventAttachmentDto, CommandRecordingDto, CommandRecordingState,
    DeletedMeetingDto, ExportFormat, ExportedMeetingDto, MeetingAnalysisDto,
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
    ArtifactKind, AudioArtifact, JobKind, JobStatus, Meeting, MeetingStatus, ModelRun,
    ProcessingJob, RawAudioRetentionPolicy as DomainRawAudioRetentionPolicy, RecordingSession,
    RecordingSource, RecordingStatus, SourceChannel, TranscriptVersion,
};
use curiosity_store::{
    AppSettings, CompletedAudioArtifact, OllamaConnectionTestEvidence,
    PendingDeleteFinalizationReport, RecoverableArtifact, Store, WhisperPathTestEvidence,
    WhisperTranscriptionCompatibilityEvidence, DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL,
};
#[cfg(feature = "whisper-rs")]
use curiosity_transcription::RealWhisperBackend;
use curiosity_transcription::{
    TranscriptionDocument, TranscriptionError, WhisperBackend, WhisperTranscriber,
    WhisperTranscriptionRequest,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::Manager;
use url::Url;

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

fn main() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(DesktopCommandState::default()))
        .on_window_event(cancel_active_recording_on_window_close);
    #[cfg(any(test, debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        desktop_snapshot,
        search_meetings,
        rename_meeting,
        correct_transcript_segment,
        export_meeting,
        export_meeting_json,
        delete_meeting,
        generate_summary,
        get_settings,
        save_whisper_model_path,
        save_analysis_settings,
        save_raw_audio_retention_policy,
        request_apple_calendar_access,
        attach_calendar_event_context,
        test_whisper_model_path,
        test_ollama_connection,
        audio_smoke_status,
        system_audio_smoke_recording,
        import_audio_file,
        start_microphone_recording,
        stop_microphone_recording,
        cancel_microphone_recording,
        transcribe_meeting,
        cancel_transcription,
        cancel_summary,
        seed_dev_fixture
    ]);
    #[cfg(not(any(test, debug_assertions)))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        desktop_snapshot,
        search_meetings,
        rename_meeting,
        correct_transcript_segment,
        export_meeting,
        export_meeting_json,
        delete_meeting,
        generate_summary,
        get_settings,
        save_whisper_model_path,
        save_analysis_settings,
        save_raw_audio_retention_policy,
        request_apple_calendar_access,
        attach_calendar_event_context,
        test_whisper_model_path,
        test_ollama_connection,
        audio_smoke_status,
        system_audio_smoke_recording,
        import_audio_file,
        start_microphone_recording,
        stop_microphone_recording,
        cancel_microphone_recording,
        transcribe_meeting,
        cancel_transcription,
        cancel_summary
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("failed to run Curiosity Transcripts desktop shell");
}

fn cancel_active_recording_on_window_close<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    event: &tauri::WindowEvent,
) {
    if !matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
        return;
    }

    let app = window.app_handle();
    let app_root = match app.path().app_data_dir() {
        Ok(app_root) => app_root,
        Err(error) => {
            eprintln!("failed to resolve app data directory during recording shutdown: {error}");
            return;
        }
    };
    let Some(command_state) = app.try_state::<Mutex<DesktopCommandState>>() else {
        eprintln!("failed to resolve desktop command state during recording shutdown");
        return;
    };

    let has_active_recording = match command_state.lock() {
        Ok(command_state) => command_state.active_recording.is_some(),
        Err(error) => {
            eprintln!("failed to lock desktop command state during recording shutdown: {error}");
            return;
        }
    };
    if !has_active_recording {
        return;
    }

    if let Err(error) =
        cancel_active_recording_for_shutdown(&app_root, &command_state, current_timestamp_ms())
    {
        eprintln!("failed to cancel active recording during window close: {error}");
    }
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
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    rename_meeting_for_app_root(&app_root, &snapshot_state, &meeting_id, &title)
}

#[tauri::command]
fn correct_transcript_segment(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
    segment_id: String,
    corrected_text: String,
    edited_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    correct_transcript_segment_for_app_root(
        &app_root,
        &snapshot_state,
        &meeting_id,
        &segment_id,
        &corrected_text,
        edited_at_ms,
    )
}

#[tauri::command]
fn export_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
    format: ExportFormat,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let export_state = export_meeting_command_state_for_app_root(&app_root, &meeting_id, format)?;
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.last_export = Some(export_state);
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
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
    let export_state = export_meeting_json_command_state_for_app_root(&app_root, &meeting_id)?;
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.last_export = Some(export_state);
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
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
    let has_active_recording = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.active_recording_matches(&meeting_id)
    };
    let delete_state =
        delete_meeting_command_state_for_app_root(&app_root, &meeting_id, has_active_recording)?;
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.last_delete = Some(delete_state);
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
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
    let (job, snapshot) = match start_summary_job_for_app_root(
        &app_root,
        state.inner(),
        &meeting_id,
        current_timestamp_ms(),
    ) {
        Ok(started) => started,
        Err(_) => {
            let snapshot_state = {
                let command_state = state.lock().map_err(|error| error.to_string())?;
                command_state.snapshot_state()
            };
            return desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state);
        }
    };
    spawn_summary_job(app, app_root, job, meeting_id);
    Ok(snapshot)
}

fn spawn_summary_job(
    app: tauri::AppHandle,
    app_root: PathBuf,
    job: CommandJobView,
    meeting_id: String,
) {
    std::thread::spawn(move || {
        let command_state = app.state::<Mutex<DesktopCommandState>>();
        if let Err(error) = finish_summary_job_for_app_root(
            &app_root,
            command_state.inner(),
            job,
            &meeting_id,
            current_timestamp_ms(),
        ) {
            eprintln!("summary job failed: {error}");
        }
    });
}

fn finish_summary_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    job: CommandJobView,
    meeting_id: &str,
    created_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let command = generate_summary_for_app_root_with_cancellation(
        app_root,
        meeting_id,
        created_at_ms,
        || summary_job_cancel_requested(command_state, &job.id),
    );
    let (finish_state, last_error) = finish_state_for_summary(&command);
    let snapshot_state = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        match &command {
            Ok(Some(command)) => {
                command_state.finish_summary_job(&job, finish_state);
                command_state.last_analysis = Some(command.clone());
            }
            Ok(None) => {
                command_state.finish_summary_job(&job, finish_state);
            }
            Err(_) => {
                command_state.finish_summary_job(&job, finish_state);
            }
        }
        command_state.snapshot_state()
    };
    persist_summary_job_finish(
        app_root,
        &job.id,
        finish_state,
        created_at_ms,
        last_error.as_deref(),
    )?;
    command?;
    desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state)
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
fn save_raw_audio_retention_policy(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    raw_audio_retention_policy: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    save_raw_audio_retention_policy_for_app_root(&app_root, raw_audio_retention_policy)?;
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn request_apple_calendar_access(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let authorization_status = request_apple_calendar_full_access();
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        #[cfg(test)]
        {
            let mut command_state = command_state;
            command_state.last_calendar_authorization_status = Some(authorization_status);
            command_state.snapshot_state()
        }
        #[cfg(not(test))]
        {
            let _ = authorization_status;
            command_state.snapshot_state()
        }
    };
    desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state)
}

#[tauri::command]
fn attach_calendar_event_context(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    meeting_id: String,
    event_id: String,
    privacy_confirmed: bool,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let snapshot_state = {
        let command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    attach_calendar_event_context_for_app_root(
        &app_root,
        &snapshot_state,
        &meeting_id,
        &event_id,
        privacy_confirmed,
        current_timestamp_ms(),
    )
}

#[tauri::command]
fn test_whisper_model_path(
    app: tauri::AppHandle,
    path: String,
) -> Result<WhisperModelPathTestView, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    test_whisper_model_path_for_app_root(&app_root, path, current_timestamp_ms())
}

#[tauri::command]
fn test_ollama_connection(
    app: tauri::AppHandle,
    base_url: String,
    model: String,
) -> Result<OllamaConnectionTestView, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    test_ollama_connection_for_app_root(
        &app_root,
        base_url,
        model,
        &UreqOllamaHttpTransport,
        current_timestamp_ms(),
    )
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
        command_state.begin_recording_start()?;
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
        command_state.finish_recording_start();
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
fn cancel_microphone_recording(
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
            .ok_or_else(|| "Start a desktop recording before canceling.".to_string())?
    };
    let recording = cancel_active_microphone_recording(
        &app_root,
        active,
        current_timestamp_ms(),
        "user canceled active recording",
    );
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
    if let Some(snapshot) = transcription_readiness_failure_snapshot_for_app_root(
        &app_root,
        state.inner(),
        &meeting_id,
        &settings,
    )? {
        return Ok(snapshot);
    }
    let started_at_ms = current_timestamp_ms();
    let (job, snapshot) = match start_transcription_job_for_app_root(
        &app_root,
        state.inner(),
        &meeting_id,
        started_at_ms,
    ) {
        Ok(started) => started,
        Err(_) => {
            let snapshot_state = {
                let command_state = state.lock().map_err(|error| error.to_string())?;
                command_state.snapshot_state()
            };
            return desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state);
        }
    };

    #[cfg(feature = "whisper-rs")]
    {
        spawn_transcription_job(
            app,
            TranscriptionJobWork {
                app_root,
                job,
                meeting_id,
                model_path: PathBuf::from(model_path),
                model_name,
                backend: RealWhisperBackend,
                created_at_ms: started_at_ms,
            },
        );
        Ok(snapshot)
    }

    #[cfg(not(feature = "whisper-rs"))]
    {
        spawn_transcription_job(
            app,
            TranscriptionJobWork {
                app_root,
                job,
                meeting_id,
                model_path: PathBuf::from(model_path),
                model_name,
                backend: BackendUnavailableWhisperBackend,
                created_at_ms: started_at_ms,
            },
        );
        Ok(snapshot)
    }
}

struct TranscriptionJobWork<B> {
    app_root: PathBuf,
    job: CommandJobView,
    meeting_id: String,
    model_path: PathBuf,
    model_name: String,
    backend: B,
    created_at_ms: u64,
}

fn spawn_transcription_job<B>(app: tauri::AppHandle, work: TranscriptionJobWork<B>)
where
    B: WhisperBackend + Send + 'static,
{
    std::thread::spawn(move || {
        let command_state = app.state::<Mutex<DesktopCommandState>>();
        if let Err(error) = finish_transcription_job_for_app_root(
            &work.app_root,
            command_state.inner(),
            work.job,
            &work.meeting_id,
            work.model_path,
            work.model_name,
            work.backend,
            work.created_at_ms,
        ) {
            eprintln!("transcription job failed: {error}");
        }
    });
}

#[tauri::command]
fn cancel_transcription(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    job_id: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    cancel_transcription_job_for_app_root(&app_root, state.inner(), &job_id)
}

#[tauri::command]
fn cancel_summary(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    job_id: String,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    cancel_summary_job_for_app_root(&app_root, state.inner(), &job_id)
}

#[tauri::command]
fn import_audio_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DesktopCommandState>>,
    source_path: String,
    title: Option<String>,
) -> Result<DesktopSnapshot, String> {
    let app_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.begin_import_audio()?;
        command_state.snapshot_state()
    };
    let result = import_audio_file_recording_for_app_root(
        &app_root,
        &snapshot_state,
        source_path,
        title,
        current_timestamp_ms(),
    );
    let snapshot_state = {
        let mut command_state = state.lock().map_err(|error| error.to_string())?;
        command_state.finish_import_audio();
        if let Ok(recording) = &result {
            command_state.last_recording = Some(recording.clone());
            command_state.last_transcription = None;
        }
        command_state.snapshot_state()
    };
    match result {
        Ok(_) => desktop_snapshot_for_app_root_with_state(&app_root, &snapshot_state),
        Err(error) => Err(error),
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
    let (store, finalized_deletes) = if command_state.active_recording.is_some() {
        (open_store(app_root)?, Vec::new())
    } else if command_state.last_delete.is_some() {
        (open_store_with_startup_repair(app_root)?, Vec::new())
    } else {
        open_store_with_startup_repair_report(app_root)?
    };
    let finalized_delete_state = finalized_deletes
        .into_iter()
        .last()
        .map(delete_command_state_from_pending_finalization);
    let recovered_transcription_job =
        if command_state.active_recording.is_none() && command_state.transcription_job.is_none() {
            store
                .recover_active_transcription_jobs(
                    current_timestamp_ms(),
                    "transcription worker was not running after app restart",
                )
                .map_err(|error| error.to_string())?
                .into_iter()
                .next()
                .map(command_job_from_processing_job)
        } else {
            None
        };
    let recovered_summary_job = if command_state.summary_job.is_none() {
        store
            .recover_active_summary_jobs(
                current_timestamp_ms(),
                "summary worker was not running after app restart",
            )
            .map_err(|error| error.to_string())?
            .into_iter()
            .next()
            .map(command_job_from_processing_job)
    } else {
        None
    };
    let settings = store.app_settings().map_err(|error| error.to_string())?;
    let meeting_summaries = list_meetings_dto(&store).map_err(|error| error.to_string())?;
    let mut meetings = Vec::with_capacity(meeting_summaries.len());

    for summary in meeting_summaries {
        let detail =
            meeting_detail_dto(&store, &summary.meeting_id).map_err(|error| error.to_string())?;
        let analysis = store
            .current_analysis_result(&summary.meeting_id)
            .map_err(|error| error.to_string())?;
        let raw_audio_retention = store
            .latest_recording_session_raw_audio_retention_policy_for_meeting(&summary.meeting_id)
            .map_err(|error| error.to_string())?
            .map(raw_audio_retention_policy_view)
            .unwrap_or(RawAudioRetentionPolicy::Retain);
        let calendar_attachment = store
            .meeting_calendar_context(&summary.meeting_id)
            .map_err(|error| error.to_string())?
            .map(MeetingCalendarAttachmentView::from_store);
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
                original_text: segment.original_text,
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
                raw_audio_retention,
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
            calendar_attachment,
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
    let has_system_audio_transcript = meetings_have_system_audio_transcript(&meetings);

    Ok(DesktopSnapshot {
        loading: false,
        command_surface: CommandSurfaceState {
            ready: true,
            detail: "Connected to local desktop commands.".to_string(),
        },
        meetings,
        selected_meeting_id,
        recording: recording_snapshot(app_root, command_state),
        model: model_status_from_settings(&settings),
        setup_guidance: setup_guidance_from_settings(&settings),
        model_setup_options: model_setup_options(),
        calendar_context: calendar_context_snapshot(
            command_state.last_calendar_authorization_status,
        ),
        settings: app_settings_view(settings),
        capture: CaptureStatus {
            microphone: microphone_capture_state(command_state),
            system_audio: system_audio_capture_state(command_state, has_system_audio_transcript),
        },
        transcription: command_state.last_transcription.clone(),
        transcription_job: command_state
            .transcription_job
            .clone()
            .or(recovered_transcription_job),
        export_command: command_state.last_export.clone().unwrap_or_default(),
        delete_command: command_state
            .last_delete
            .clone()
            .or(finalized_delete_state)
            .unwrap_or_default(),
        analysis_command: command_state.last_analysis.clone(),
        summary_job: command_state.summary_job.clone().or(recovered_summary_job),
    })
}

fn open_store(app_root: &Path) -> Result<Store, String> {
    open_store_for_app_root(app_root, false).map(|(store, _)| store)
}

fn open_store_with_startup_repair(app_root: &Path) -> Result<Store, String> {
    open_store_with_startup_repair_report(app_root).map(|(store, _)| store)
}

fn open_store_with_startup_repair_report(
    app_root: &Path,
) -> Result<(Store, Vec<PendingDeleteFinalizationReport>), String> {
    open_store_for_app_root(app_root, true)
}

fn open_store_for_app_root(
    app_root: &Path,
    repair_startup: bool,
) -> Result<(Store, Vec<PendingDeleteFinalizationReport>), String> {
    std::fs::create_dir_all(app_root).map_err(|error| error.to_string())?;
    let store = Store::open(app_root.join("curiosity.sqlite3"), app_root.to_path_buf())
        .map_err(|error| error.to_string())?;
    store.migrate().map_err(|error| error.to_string())?;
    let finalized_deletes = if repair_startup {
        store.repair_startup().map_err(|error| error.to_string())?;
        store
            .finalize_pending_raw_audio_retention_cleanup()
            .map_err(|error| error.to_string())?;
        store
            .finalize_pending_delete_intents()
            .map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    Ok((store, finalized_deletes))
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
    let ollama_base_url = match ollama_base_url.trim() {
        "" => DEFAULT_OLLAMA_BASE_URL,
        value => value,
    };
    let ollama_model = match ollama_model.trim() {
        "" => DEFAULT_OLLAMA_MODEL,
        value => value,
    };
    let ollama_model = canonical_local_ollama_model_tag(ollama_model);
    validate_local_ollama_model(&ollama_model).map_err(|error| error.to_string())?;
    local_ollama_endpoint(ollama_base_url, "/api/tags").map_err(|error| error.to_string())?;
    let store = open_store(app_root)?;
    store
        .save_analysis_settings(ollama_base_url, &ollama_model)
        .map(app_settings_view)
        .map_err(|error| error.to_string())
}

fn save_raw_audio_retention_policy_for_app_root(
    app_root: &Path,
    raw_audio_retention_policy: String,
) -> Result<AppSettingsView, String> {
    let store = open_store(app_root)?;
    store
        .save_raw_audio_retention_policy(&raw_audio_retention_policy)
        .map(app_settings_view)
        .map_err(|error| error.to_string())
}

fn test_whisper_model_path_for_app_root(
    app_root: &Path,
    path: String,
    tested_at_ms: u64,
) -> Result<WhisperModelPathTestView, String> {
    let result = test_whisper_model_path_value(&path);
    let evidence = WhisperPathTestEvidence {
        tested_path: path.trim().to_string(),
        tested_at_ms,
        state: result.state.clone(),
        file_size_bytes: result.file_size_bytes,
        sha256: result.sha256.clone(),
        failure_detail: if result.state == "Valid" {
            None
        } else {
            Some(result.message.clone())
        },
    };
    open_store(app_root)?
        .save_whisper_path_test_evidence(&evidence)
        .map_err(|error| format!("persist Whisper path test evidence: {error}"))?;
    Ok(result)
}

fn test_ollama_connection_for_app_root<T>(
    app_root: &Path,
    base_url: String,
    model: String,
    transport: &T,
    tested_at_ms: u64,
) -> Result<OllamaConnectionTestView, String>
where
    T: OllamaHttpTransport,
{
    let result = test_ollama_connection_value(&base_url, &model, transport);
    let evidence = OllamaConnectionTestEvidence {
        base_url: base_url.trim().to_string(),
        requested_model: canonical_local_ollama_model_tag(&model),
        tested_at_ms,
        state: result.state.clone(),
        selected_local_model_tag: result.selected_local_model_tag.clone(),
        installed_local_models: result.installed_local_models.clone(),
        pull_command: result.pull_command.clone(),
        failure_detail: if result.state == "Available" {
            None
        } else {
            Some(result.message.clone())
        },
    };
    open_store(app_root)?
        .save_ollama_connection_test_evidence(&evidence)
        .map_err(|error| format!("persist Ollama connection test evidence: {error}"))?;
    Ok(result)
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
    command_state: &DesktopCommandSnapshotState,
    meeting_id: &str,
    title: &str,
) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    rename_meeting_command(&store, meeting_id, title).map_err(|error| error.to_string())?;
    drop(store);
    desktop_snapshot_for_app_root_with_state(app_root, command_state)
}

fn attach_calendar_event_context_for_app_root(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
    meeting_id: &str,
    event_id: &str,
    privacy_confirmed: bool,
    attached_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let events =
        calendar_context_snapshot(command_state.last_calendar_authorization_status).upcoming_events;
    attach_calendar_event_context_for_app_root_with_events(
        app_root,
        command_state,
        meeting_id,
        event_id,
        privacy_confirmed,
        attached_at_ms,
        events,
    )
}

fn attach_calendar_event_context_for_app_root_with_events(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
    meeting_id: &str,
    event_id: &str,
    privacy_confirmed: bool,
    attached_at_ms: u64,
    events: Vec<CalendarContextEventView>,
) -> Result<DesktopSnapshot, String> {
    let event = events
        .into_iter()
        .find(|event| event.id == event_id)
        .ok_or_else(|| "Calendar event is no longer available for attachment.".to_string())?;
    let store = open_store(app_root)?;
    attach_calendar_event_context_command(
        &store,
        meeting_id,
        calendar_event_attachment_dto(event),
        privacy_confirmed,
        attached_at_ms,
    )
    .map_err(|error| error.to_string())?;
    drop(store);
    desktop_snapshot_for_app_root_with_state(app_root, command_state)
}

fn calendar_event_attachment_dto(event: CalendarContextEventView) -> CalendarEventAttachmentDto {
    CalendarEventAttachmentDto {
        id: event.id,
        title: event.title,
        calendar_title: event.calendar_title,
        starts_at_ms: event.starts_at_ms,
        ends_at_ms: event.ends_at_ms,
        is_all_day: event.is_all_day,
        is_recurring: event.is_recurring,
        privacy: event.privacy,
        overlap_state: event.overlap_state,
        attachable: event.attachable,
    }
}

fn correct_transcript_segment_for_app_root(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
    meeting_id: &str,
    segment_id: &str,
    corrected_text: &str,
    edited_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let store = open_store(app_root)?;
    correct_transcript_segment_command(
        &store,
        meeting_id,
        segment_id,
        corrected_text,
        edited_at_ms,
    )
    .map_err(|error| error.to_string())?;
    drop(store);
    desktop_snapshot_for_app_root_with_state(app_root, command_state)
}

#[cfg(test)]
fn export_meeting_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
    format: ExportFormat,
) -> Result<DesktopSnapshot, String> {
    let export_state = export_meeting_command_state_for_app_root(app_root, meeting_id, format)?;
    command_state.last_export = Some(export_state);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

#[cfg(test)]
fn export_meeting_json_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
) -> Result<DesktopSnapshot, String> {
    let export_state = export_meeting_json_command_state_for_app_root(app_root, meeting_id)?;
    command_state.last_export = Some(export_state);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn export_meeting_json_command_state_for_app_root(
    app_root: &Path,
    meeting_id: &str,
) -> Result<ExportCommandState, String> {
    export_meeting_command_state_for_app_root(app_root, meeting_id, ExportFormat::Json)
}

fn export_meeting_command_state_for_app_root(
    app_root: &Path,
    meeting_id: &str,
    format: ExportFormat,
) -> Result<ExportCommandState, String> {
    let store = open_store(app_root)?;
    let settings = store.app_settings().map_err(|error| error.to_string())?;
    let export_root = export_root_for_settings(app_root, &settings);
    let export_result = if format == ExportFormat::Json {
        export_meeting_json_command(&store, meeting_id, &export_root)
    } else {
        export_meeting_command(&store, meeting_id, format, &export_root)
    };
    let export_state = match export_result {
        Ok(exported) => ExportCommandState::exported(exported),
        Err(error) => ExportCommandState::failed(meeting_id, format, error.to_string()),
    };
    Ok(export_state)
}

#[cfg(test)]
fn delete_meeting_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    meeting_id: &str,
) -> Result<DesktopSnapshot, String> {
    let delete_state = delete_meeting_command_state_for_app_root(
        app_root,
        meeting_id,
        command_state.active_recording_matches(meeting_id),
    )?;
    command_state.last_delete = Some(delete_state);
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn delete_meeting_command_state_for_app_root(
    app_root: &Path,
    meeting_id: &str,
    has_active_recording: bool,
) -> Result<DeleteCommandState, String> {
    if has_active_recording {
        return Ok(DeleteCommandState::failed(
            meeting_id,
            "Cannot delete a meeting while it has an active recording.".to_string(),
        ));
    }

    let store = open_store(app_root)?;
    let delete_state = match delete_meeting_command(&store, meeting_id) {
        Ok(deleted) => DeleteCommandState::deleted(deleted),
        Err(error) => DeleteCommandState::failed(meeting_id, error.to_string()),
    };
    Ok(delete_state)
}

fn generate_summary_for_app_root_with_cancellation(
    app_root: &Path,
    meeting_id: &str,
    created_at_ms: u64,
    is_cancelled: impl Fn() -> bool,
) -> Result<Option<AnalysisCommandView>, String> {
    let settings = app_settings_for_app_root(app_root)?;
    let client =
        LocalOllamaTextClient::new(settings.ollama_base_url.clone(), UreqOllamaHttpTransport);
    let model_name = canonical_local_ollama_model_tag(&settings.ollama_model);
    generate_summary_command_for_app_root_with_client_and_cancellation(
        app_root,
        meeting_id,
        client,
        model_name,
        created_at_ms,
        is_cancelled,
    )
}

#[cfg(test)]
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
    generate_summary_command_for_app_root_with_client_and_cancellation(
        app_root,
        meeting_id,
        client,
        model_name,
        created_at_ms,
        || false,
    )
    .map(|command| command.expect("non-cancelable summary command cannot be canceled"))
}

fn generate_summary_command_for_app_root_with_client_and_cancellation<C>(
    app_root: &Path,
    meeting_id: &str,
    client: C,
    model_name: impl Into<String>,
    created_at_ms: u64,
    is_cancelled: impl Fn() -> bool,
) -> Result<Option<AnalysisCommandView>, String>
where
    C: ProviderTextClient,
{
    let store = open_store(app_root)?;
    let analyzer = OllamaAnalyzer::new(client, model_name, "summary-v1");
    generate_summary_command_with_cancellation(
        &store,
        &analyzer,
        meeting_id,
        created_at_ms,
        is_cancelled,
    )
    .map(|command| command.map(AnalysisCommandView::from_command))
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

#[cfg(test)]
fn finish_summary_job_for_app_root_with_client<C>(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    job: CommandJobView,
    meeting_id: &str,
    client: C,
    model_name: impl Into<String>,
    created_at_ms: u64,
) -> Result<DesktopSnapshot, String>
where
    C: ProviderTextClient,
{
    let command = generate_summary_command_for_app_root_with_client_and_cancellation(
        app_root,
        meeting_id,
        client,
        model_name,
        created_at_ms,
        || summary_job_cancel_requested(command_state, &job.id),
    );
    let (finish_state, last_error) = finish_state_for_summary(&command);
    let snapshot_state = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        match &command {
            Ok(Some(command)) => {
                command_state.finish_summary_job(&job, finish_state);
                command_state.last_analysis = Some(command.clone());
            }
            Ok(None) | Err(_) => {
                command_state.finish_summary_job(&job, finish_state);
            }
        }
        command_state.snapshot_state()
    };
    persist_summary_job_finish(
        app_root,
        &job.id,
        finish_state,
        created_at_ms,
        last_error.as_deref(),
    )?;
    command?;
    desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state)
}

fn begin_summary_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    meeting_id: &str,
    started_at_ms: u64,
) -> Result<CommandJobView, String> {
    let job = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state
            .begin_summary_job(meeting_id, started_at_ms)
            .map_err(|job| format!("{} already owns summary for {}", job.id, job.meeting_id))?
    };

    let store = match open_store(app_root) {
        Ok(store) => store,
        Err(error) => {
            let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
            command_state.finish_summary_job(&job, CommandJobFinishState::Failed);
            return Err(error);
        }
    };
    let active_job = match store.active_summary_job_for_meeting(meeting_id) {
        Ok(active_job) => active_job,
        Err(error) => {
            let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
            command_state.finish_summary_job(&job, CommandJobFinishState::Failed);
            return Err(error.to_string());
        }
    };
    if let Some(active_job) = active_job {
        let recovered_job = match store
            .recover_processing_job(
                &active_job.id,
                started_at_ms,
                "summary worker was not running after app restart",
            )
            .and_then(|_| store.processing_job(&active_job.id))
        {
            Ok(recovered_job) => recovered_job,
            Err(error) => {
                let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
                command_state.finish_summary_job(&job, CommandJobFinishState::Failed);
                return Err(error.to_string());
            }
        };
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.summary_job = Some(command_job_from_processing_job(recovered_job));
        return Err(format!(
            "{} already owns summary for {}",
            active_job.id, active_job.meeting_id
        ));
    }

    let durable_job = processing_job_from_command_job(&job);
    if let Err(error) = store.insert_processing_job(&durable_job) {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.finish_summary_job(&job, CommandJobFinishState::Failed);
        return Err(error.to_string());
    }

    Ok(job)
}

fn start_summary_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    meeting_id: &str,
    started_at_ms: u64,
) -> Result<(CommandJobView, DesktopSnapshot), String> {
    let job = begin_summary_job_for_app_root(app_root, command_state, meeting_id, started_at_ms)?;
    let snapshot_state = {
        let command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    let snapshot = match desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
            command_state.finish_summary_job(&job, CommandJobFinishState::Failed);
            let _ = persist_summary_job_finish(
                app_root,
                &job.id,
                CommandJobFinishState::Failed,
                started_at_ms,
                Some(&error),
            );
            return Err(error);
        }
    };
    Ok((job, snapshot))
}

fn cancel_summary_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    job_id: &str,
) -> Result<DesktopSnapshot, String> {
    let snapshot_state = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.request_summary_cancel(job_id)?;
        command_state.snapshot_state()
    };
    persist_summary_job_cancel_request(app_root, job_id)?;
    desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state)
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
    importing_audio: bool,
    last_recording: Option<CommandRecordingDto>,
    last_transcription: Option<TranscriptionCommandView>,
    last_export: Option<ExportCommandState>,
    last_delete: Option<DeleteCommandState>,
    last_analysis: Option<AnalysisCommandView>,
    last_calendar_authorization_status: Option<AppleCalendarAuthorizationStatus>,
    transcription_job: Option<CommandJobView>,
    summary_job: Option<CommandJobView>,
}

impl DesktopCommandState {
    fn snapshot_state(&self) -> DesktopCommandSnapshotState {
        DesktopCommandSnapshotState {
            active_recording: self.active_recording.as_ref().map(|recording| {
                ActiveDesktopRecordingSnapshot {
                    meeting_id: recording.meeting_id.clone(),
                    recording_id: recording.recording_id.clone(),
                    captures_system_audio: recording.streams.contains(&StreamKind::SystemAudio),
                    raw_audio_retention_policy: recording.raw_audio_retention_policy,
                }
            }),
            last_recording: self.last_recording.clone(),
            last_transcription: self.last_transcription.clone(),
            last_export: self.last_export.clone(),
            last_delete: self.last_delete.clone(),
            last_analysis: self.last_analysis.clone(),
            last_calendar_authorization_status: self.last_calendar_authorization_status,
            transcription_job: self.transcription_job.clone(),
            summary_job: self.summary_job.clone(),
        }
    }

    fn active_recording_matches(&self, meeting_id: &str) -> bool {
        self.active_recording
            .as_ref()
            .map(|recording| recording.meeting_id == meeting_id)
            .unwrap_or(false)
    }

    fn begin_recording_start(&mut self) -> Result<(), String> {
        if self.active_recording.is_some() || self.starting_recording {
            return Err("Stop the active recording before starting another one.".to_string());
        }
        if self.importing_audio {
            return Err("Finish the active audio import before starting a recording.".to_string());
        }
        self.starting_recording = true;
        Ok(())
    }

    fn finish_recording_start(&mut self) {
        self.starting_recording = false;
    }

    fn begin_import_audio(&mut self) -> Result<(), String> {
        if self.active_recording.is_some() {
            return Err("Stop the active recording before importing audio.".to_string());
        }
        if self.starting_recording {
            return Err("Finish recording startup before importing audio.".to_string());
        }
        if self.importing_audio {
            return Err("Finish the active audio import before importing another WAV.".to_string());
        }
        self.importing_audio = true;
        Ok(())
    }

    fn finish_import_audio(&mut self) {
        self.importing_audio = false;
    }

    fn begin_transcription_job(
        &mut self,
        meeting_id: &str,
        started_at_ms: u64,
    ) -> Result<CommandJobView, CommandJobView> {
        begin_job(
            &mut self.transcription_job,
            CommandJobKind::Transcription,
            meeting_id,
            started_at_ms,
        )
    }

    fn finish_transcription_job(&mut self, job: &CommandJobView, state: CommandJobFinishState) {
        finish_job(&mut self.transcription_job, job, state);
    }

    fn request_transcription_cancel(&mut self, job_id: &str) -> Result<(), String> {
        request_job_cancel(
            &mut self.transcription_job,
            job_id,
            CommandJobKind::Transcription,
        )
    }

    fn transcription_job_cancel_requested(&self, job_id: &str) -> bool {
        job_cancel_requested(&self.transcription_job, job_id)
    }

    fn begin_summary_job(
        &mut self,
        meeting_id: &str,
        started_at_ms: u64,
    ) -> Result<CommandJobView, CommandJobView> {
        begin_job(
            &mut self.summary_job,
            CommandJobKind::Summary,
            meeting_id,
            started_at_ms,
        )
    }

    fn finish_summary_job(&mut self, job: &CommandJobView, state: CommandJobFinishState) {
        finish_job(&mut self.summary_job, job, state);
    }

    fn request_summary_cancel(&mut self, job_id: &str) -> Result<(), String> {
        request_job_cancel(&mut self.summary_job, job_id, CommandJobKind::Summary)
    }

    fn summary_job_cancel_requested(&self, job_id: &str) -> bool {
        job_cancel_requested(&self.summary_job, job_id)
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
    last_calendar_authorization_status: Option<AppleCalendarAuthorizationStatus>,
    transcription_job: Option<CommandJobView>,
    summary_job: Option<CommandJobView>,
}

#[derive(Clone)]
struct ActiveDesktopRecordingSnapshot {
    meeting_id: String,
    recording_id: String,
    captures_system_audio: bool,
    raw_audio_retention_policy: RawAudioRetentionPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandJobView {
    id: String,
    kind: CommandJobKind,
    meeting_id: String,
    state: CommandJobState,
    cancel_requested: bool,
    started_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
enum CommandJobKind {
    Transcription,
    Summary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
enum CommandJobState {
    Running,
    CancelRequested,
    Complete,
    Failed,
    Recovery,
    Retry,
    Canceled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandJobFinishState {
    Complete,
    Failed,
    Canceled,
}

impl CommandJobView {
    fn running(kind: CommandJobKind, meeting_id: &str, started_at_ms: u64) -> Self {
        let job_kind = match kind {
            CommandJobKind::Transcription => "transcription",
            CommandJobKind::Summary => "summary",
        };
        Self {
            id: format!("{job_kind}-{meeting_id}-{started_at_ms}"),
            kind,
            meeting_id: meeting_id.to_string(),
            state: CommandJobState::Running,
            cancel_requested: false,
            started_at_ms,
            last_error: None,
        }
    }

    fn is_active(&self) -> bool {
        matches!(
            self.state,
            CommandJobState::Running | CommandJobState::CancelRequested
        )
    }
}

fn begin_job(
    slot: &mut Option<CommandJobView>,
    kind: CommandJobKind,
    meeting_id: &str,
    started_at_ms: u64,
) -> Result<CommandJobView, CommandJobView> {
    if let Some(job) = slot.as_ref().filter(|job| job.is_active()) {
        return Err(job.clone());
    }
    let job = CommandJobView::running(kind, meeting_id, started_at_ms);
    *slot = Some(job.clone());
    Ok(job)
}

fn finish_job(
    slot: &mut Option<CommandJobView>,
    job: &CommandJobView,
    state: CommandJobFinishState,
) {
    if let Some(active) = slot.as_mut().filter(|active| active.id == job.id) {
        active.state = match state {
            CommandJobFinishState::Complete => CommandJobState::Complete,
            CommandJobFinishState::Failed => CommandJobState::Failed,
            CommandJobFinishState::Canceled => CommandJobState::Canceled,
        };
        active.cancel_requested = false;
    }
}

fn job_cancel_requested(slot: &Option<CommandJobView>, job_id: &str) -> bool {
    slot.as_ref()
        .filter(|job| job.id == job_id)
        .map(|job| job.cancel_requested)
        .unwrap_or(false)
}

fn transcription_job_cancel_requested(
    command_state: &Mutex<DesktopCommandState>,
    job_id: &str,
) -> bool {
    command_state
        .lock()
        .map(|state| state.transcription_job_cancel_requested(job_id))
        .unwrap_or(true)
}

fn summary_job_cancel_requested(command_state: &Mutex<DesktopCommandState>, job_id: &str) -> bool {
    command_state
        .lock()
        .map(|state| state.summary_job_cancel_requested(job_id))
        .unwrap_or(true)
}

fn request_job_cancel(
    slot: &mut Option<CommandJobView>,
    job_id: &str,
    kind: CommandJobKind,
) -> Result<(), String> {
    let Some(job) = slot.as_mut().filter(|job| job.is_active()) else {
        return Err(format!("No active {kind:?} job to cancel."));
    };
    if job.id != job_id {
        return Err(format!(
            "{} already owns {kind:?} for {}",
            job.id, job.meeting_id
        ));
    }
    job.state = CommandJobState::CancelRequested;
    job.cancel_requested = true;
    Ok(())
}

struct ActiveDesktopRecording {
    meeting_id: String,
    recording_id: String,
    streams: Vec<StreamKind>,
    raw_audio_retention_policy: RawAudioRetentionPolicy,
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

#[cfg(test)]
fn import_audio_file_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    source_path: String,
    title: Option<String>,
    imported_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let snapshot_state = command_state.snapshot_state();
    let recording = import_audio_file_recording_for_app_root(
        app_root,
        &snapshot_state,
        source_path,
        title,
        imported_at_ms,
    )?;
    command_state.last_recording = Some(recording);
    command_state.last_transcription = None;
    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn import_audio_file_recording_for_app_root(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
    source_path: String,
    title: Option<String>,
    imported_at_ms: u64,
) -> Result<CommandRecordingDto, String> {
    if command_state.active_recording.is_some() {
        return Err("Stop the active recording before importing audio.".to_string());
    }

    let source_path = validate_import_source_path(&source_path)?;
    let sample_rate_hz = validate_wav_header(&source_path)?;
    let meeting_id = format!("meeting-{imported_at_ms}");
    let recording_id = format!("recording-{imported_at_ms}");
    let title = title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Imported WAV".to_string());
    let relative_path = imported_artifact_relative_path(&meeting_id, &recording_id);
    let temp_relative_path = imported_temp_artifact_relative_path(&meeting_id, &recording_id);
    let destination_path = app_root.join(&relative_path);
    let temp_destination_path = app_root.join(&temp_relative_path);
    let session_dir = destination_path
        .parent()
        .ok_or_else(|| "Imported WAV destination path is invalid.".to_string())?
        .to_path_buf();
    let store = open_store(app_root)?;
    let raw_audio_retention_policy = store
        .app_settings()
        .map_err(|error| error.to_string())?
        .raw_audio_retention_policy;
    let mut finalized_destination = false;

    let import_result = (|| {
        std::fs::create_dir_all(&session_dir).map_err(|error| {
            format!("Imported WAV private directory could not be created: {error}")
        })?;
        if destination_path.exists() {
            return Err("Imported WAV destination already exists.".to_string());
        }
        let _ = std::fs::remove_file(&temp_destination_path);
        std::fs::copy(&source_path, &temp_destination_path).map_err(|error| {
            format!("Imported WAV could not be copied to private storage: {error}")
        })?;
        let sha256 = sha256_for_readable_file(&temp_destination_path)
            .map_err(|error| format!("Imported WAV private copy could not be hashed: {error}"))?;
        std::fs::rename(&temp_destination_path, &destination_path).map_err(|error| {
            format!("Imported WAV could not be finalized in private storage: {error}")
        })?;
        finalized_destination = true;

        let mut meeting = Meeting::new_manual(&meeting_id, title, imported_at_ms);
        let session = RecordingSession::start(
            &recording_id,
            &meeting_id,
            RecordingSource::Imported,
            imported_at_ms,
            sample_rate_hz,
        )
        .with_raw_audio_retention_policy(raw_audio_retention_policy);
        meeting
            .start_recording(&session)
            .map_err(|error| error.to_string())?;
        let artifact = AudioArtifact::new_private(
            imported_artifact_id(&recording_id),
            &recording_id,
            ArtifactKind::Imported,
            &relative_path,
            &sha256,
        );
        store
            .insert_recording_start_with_artifacts(
                &meeting,
                &session,
                std::slice::from_ref(&artifact),
            )
            .map_err(|error| error.to_string())?;
        if let Err(error) = store.complete_recording_session_with_artifacts(
            &meeting_id,
            &recording_id,
            imported_at_ms,
            RecordingSource::Imported,
            &[CompletedAudioArtifact {
                artifact_id: artifact.id,
                sha256,
            }],
        ) {
            let delete_error = store.delete_meeting(&meeting_id).err();
            let mut message = error.to_string();
            if let Some(delete_error) = delete_error {
                message.push_str(&format!(
                    ". Imported WAV row cleanup also failed: {delete_error}"
                ));
            }
            return Err(message);
        }

        Ok::<(), String>(())
    })();

    if let Err(error) = import_result {
        cleanup_imported_private_copy(
            &destination_path,
            &temp_destination_path,
            &session_dir,
            finalized_destination,
        );
        return Err(error);
    }

    Ok(recording_dto_with_retention(
        &meeting_id,
        Some(recording_id),
        CommandRecordingState::Complete,
        AppPermissionState::Ready,
        microphone_storage_path(&meeting_id),
        raw_audio_retention_policy_view(raw_audio_retention_policy),
        "Imported local WAV into private app storage.",
    ))
}

fn validate_import_source_path(source_path: &str) -> Result<PathBuf, String> {
    let trimmed = source_path.trim();
    if trimmed.is_empty() {
        return Err("WAV source path is required.".to_string());
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err("WAV source file does not exist.".to_string());
    }
    let metadata = path
        .metadata()
        .map_err(|error| format!("WAV source file is not readable: {error}"))?;
    if !metadata.is_file() {
        return Err("WAV source path must be a file.".to_string());
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| !extension.eq_ignore_ascii_case("wav"))
        .unwrap_or(true)
    {
        return Err("WAV source file must have a .wav extension.".to_string());
    }
    Ok(path)
}

fn validate_wav_header(path: &Path) -> Result<u32, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("WAV source file is not readable: {error}"))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("WAV source file is not readable: {error}"))?
        .len();
    let mut riff_header = [0_u8; 12];
    file.read_exact(&mut riff_header)
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    if &riff_header[0..4] != b"RIFF" || &riff_header[8..12] != b"WAVE" {
        return Err("WAV source file has an unsupported WAV header.".to_string());
    }

    let mut sample_rate_hz = None;
    let mut has_data_chunk = false;
    loop {
        let mut chunk_header = [0_u8; 8];
        match file.read_exact(&mut chunk_header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err("WAV source file has an unsupported WAV header.".to_string()),
        }
        let chunk_size = u32::from_le_bytes([
            chunk_header[4],
            chunk_header[5],
            chunk_header[6],
            chunk_header[7],
        ]) as u64;
        ensure_wav_chunk_payload_available(&mut file, chunk_size, file_len)?;
        match &chunk_header[0..4] {
            b"fmt " => {
                if chunk_size < 16 {
                    return Err("WAV source file has an unsupported WAV header.".to_string());
                }
                let mut fmt = [0_u8; 16];
                file.read_exact(&mut fmt)
                    .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
                let audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
                let sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
                let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
                if !matches!(audio_format, 1 | 3) || sample_rate == 0 || bits_per_sample == 0 {
                    return Err("WAV source file has an unsupported WAV header.".to_string());
                }
                sample_rate_hz = Some(sample_rate);
                seek_wav_chunk_remainder(&mut file, chunk_size - 16)?;
            }
            b"data" => {
                if chunk_size == 0 {
                    return Err("WAV source file has an unsupported WAV header.".to_string());
                }
                has_data_chunk = true;
                seek_wav_chunk_remainder(&mut file, chunk_size)?;
            }
            _ => seek_wav_chunk_remainder(&mut file, chunk_size)?,
        }
        if chunk_size % 2 == 1 {
            file.seek(SeekFrom::Current(1))
                .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
        }
    }

    match (sample_rate_hz, has_data_chunk) {
        (Some(sample_rate), true) => Ok(sample_rate),
        _ => Err("WAV source file has an unsupported WAV header.".to_string()),
    }
}

fn ensure_wav_chunk_payload_available(
    file: &mut std::fs::File,
    chunk_size: u64,
    file_len: u64,
) -> Result<(), String> {
    let payload_start = file
        .stream_position()
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    let padded_size = chunk_size
        .checked_add(chunk_size % 2)
        .ok_or_else(|| "WAV source file has an unsupported WAV header.".to_string())?;
    let payload_end = payload_start
        .checked_add(padded_size)
        .ok_or_else(|| "WAV source file has an unsupported WAV header.".to_string())?;
    if payload_end > file_len {
        return Err("WAV source file has an unsupported WAV header.".to_string());
    }
    Ok(())
}

fn seek_wav_chunk_remainder(file: &mut std::fs::File, bytes: u64) -> Result<(), String> {
    let offset = i64::try_from(bytes)
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    file.seek(SeekFrom::Current(offset))
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    Ok(())
}

fn cleanup_imported_private_copy(
    destination_path: &Path,
    temp_destination_path: &Path,
    session_dir: &Path,
    remove_final_destination: bool,
) {
    let _ = std::fs::remove_file(temp_destination_path);
    if remove_final_destination {
        let _ = std::fs::remove_file(destination_path);
    }
    let _ = std::fs::remove_dir(session_dir);
    if let Some(audio_dir) = session_dir.parent() {
        let _ = std::fs::remove_dir(audio_dir);
    }
    if let Some(meeting_dir) = session_dir.parent().and_then(Path::parent) {
        let _ = std::fs::remove_dir(meeting_dir);
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
    let raw_audio_retention_policy = store
        .app_settings()
        .map_err(|error| MicrophoneStartFailure::persistence(error.to_string()))?
        .raw_audio_retention_policy;
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
    )
    .with_raw_audio_retention_policy(raw_audio_retention_policy);
    if let Err(error) = meeting.start_recording(&session) {
        return Err(metadata_persistence_failure(
            error.to_string(),
            recorder,
            started_at_ms,
            audio_root.join(&recording_id),
        ));
    }
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

    let recording = recording_dto_with_retention(
        &meeting_id,
        Some(recording_id.clone()),
        CommandRecordingState::Recording,
        AppPermissionState::Ready,
        microphone_storage_path(&meeting_id),
        raw_audio_retention_policy_view(raw_audio_retention_policy),
        "Recording locally to private app storage",
    );
    command_state.active_recording = Some(ActiveDesktopRecording {
        meeting_id,
        recording_id,
        streams,
        raw_audio_retention_policy: raw_audio_retention_policy_view(raw_audio_retention_policy),
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

#[cfg(test)]
fn cancel_microphone_recording_for_app_root(
    app_root: &Path,
    command_state: &mut DesktopCommandState,
    ended_at_ms: u64,
    reason: &str,
) -> Result<DesktopSnapshot, String> {
    let Some(active) = command_state.active_recording.take() else {
        return Err("Start a desktop recording before canceling.".to_string());
    };
    command_state.last_recording = Some(cancel_active_microphone_recording(
        app_root,
        active,
        ended_at_ms,
        reason,
    ));

    desktop_snapshot_for_app_root_with_state(app_root, &command_state.snapshot_state())
}

fn cancel_active_recording_for_shutdown(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    ended_at_ms: u64,
) -> Result<CommandRecordingDto, String> {
    let active = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state
            .active_recording
            .take()
            .ok_or_else(|| "Start a desktop recording before canceling.".to_string())?
    };
    let recording = cancel_active_microphone_recording(
        app_root,
        active,
        ended_at_ms,
        "window closed before recording completion",
    );
    {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.last_recording = Some(recording.clone());
    }
    Ok(recording)
}

fn stop_active_microphone_recording(
    app_root: &Path,
    active: ActiveDesktopRecording,
    ended_at_ms: u64,
) -> CommandRecordingDto {
    let meeting_id = active.meeting_id.clone();
    let recording_id = active.recording_id.clone();
    let raw_audio_retention_policy = active.raw_audio_retention_policy;
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
            recording_dto_with_retention(
                &meeting_id,
                Some(recording_id),
                CommandRecordingState::Interrupted,
                recording_stop_permission_state(&message),
                microphone_storage_path(&meeting_id),
                raw_audio_retention_policy,
                &format!("Recording could not be finalized: {message}"),
            )
        }
    }
}

fn cancel_active_microphone_recording(
    app_root: &Path,
    active: ActiveDesktopRecording,
    ended_at_ms: u64,
    reason: &str,
) -> CommandRecordingDto {
    let meeting_id = active.meeting_id.clone();
    let recording_id = active.recording_id.clone();
    let raw_audio_retention_policy = active.raw_audio_retention_policy;
    let stop_error = active.recorder.stop(ended_at_ms).err();
    let message = match stop_error {
        Some(error) => format!("{reason}; recorder shutdown reported: {error}"),
        None => reason.to_string(),
    };
    if let Ok(store) = open_store(app_root) {
        let _ = store.update_recording_session_status(
            &recording_id,
            RecordingStatus::Failed,
            Some(ended_at_ms),
            Some(&message),
        );
        let _ = store.update_meeting_status(&meeting_id, MeetingStatus::Failed, Some(ended_at_ms));
    }
    let manifest_path = app_root
        .join("meetings")
        .join(&meeting_id)
        .join("manifest.json");
    let _ = std::fs::remove_file(manifest_path);

    recording_dto_with_retention(
        &meeting_id,
        Some(recording_id),
        CommandRecordingState::Interrupted,
        AppPermissionState::Ready,
        microphone_storage_path(&meeting_id),
        raw_audio_retention_policy,
        &format!("Recording canceled before completion: {message}"),
    )
}

#[derive(Debug, Eq, PartialEq)]
struct CompletedAudioManifestMapping {
    completed_artifacts: Vec<CompletedAudioArtifact>,
    completed_streams: Vec<StreamKind>,
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
    let completed = completed_audio_artifacts_from_manifest(
        app_root,
        &active.meeting_id,
        &active.recording_id,
        &active.streams,
        &manifest,
    )?;
    if !completed
        .completed_streams
        .contains(&StreamKind::Microphone)
    {
        return Err("microphone recording stopped without a WAV artifact".to_string());
    }
    let recording_source = recording_source_for_streams(&completed.completed_streams);
    store
        .complete_recording_session_with_artifacts(
            &active.meeting_id,
            &active.recording_id,
            ended_at_ms,
            recording_source,
            &completed.completed_artifacts,
        )
        .map_err(|error| error.to_string())?;

    let recovery_action = if completed
        .completed_streams
        .contains(&StreamKind::SystemAudio)
    {
        "Finalized local microphone and system audio WAV artifacts."
    } else {
        "Finalized local microphone WAV artifact."
    };

    Ok(recording_dto_with_retention(
        &active.meeting_id,
        Some(active.recording_id),
        CommandRecordingState::Complete,
        AppPermissionState::Ready,
        microphone_storage_path(&active.meeting_id),
        active.raw_audio_retention_policy,
        recovery_action,
    ))
}

fn completed_audio_artifacts_from_manifest(
    app_root: &Path,
    meeting_id: &str,
    recording_id: &str,
    streams: &[StreamKind],
    manifest: &ArtifactManifest,
) -> Result<CompletedAudioManifestMapping, String> {
    let mut completed_artifacts = Vec::new();
    let mut completed_streams = Vec::new();
    for artifact in &manifest.artifacts {
        if !streams.contains(&artifact.stream) {
            return Err(format!(
                "{} artifact was not part of the active recording",
                stream_label(artifact.stream)
            ));
        }
        let relative_path =
            relative_private_artifact_path(app_root, &artifact.path, artifact.stream)?;
        let expected_path =
            artifact_relative_path_for_stream(meeting_id, recording_id, artifact.stream);
        if relative_path != expected_path {
            return Err(format!(
                "{} artifact path mismatch: expected {expected_path}, got {relative_path}",
                stream_label(artifact.stream)
            ));
        }
        completed_streams.push(artifact.stream);
        completed_artifacts.push(CompletedAudioArtifact {
            artifact_id: artifact_id_for_stream(recording_id, artifact.stream),
            sha256: artifact.sha256.clone(),
        });
    }
    Ok(CompletedAudioManifestMapping {
        completed_artifacts,
        completed_streams,
    })
}

fn relative_private_artifact_path(
    app_root: &Path,
    path: &Path,
    stream: StreamKind,
) -> Result<String, String> {
    let relative_path = path.strip_prefix(app_root).map_err(|_| {
        format!(
            "{} artifact was written outside private app storage",
            stream_label(stream)
        )
    })?;
    if relative_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "{} artifact was written outside private app storage",
            stream_label(stream)
        ));
    }
    Ok(relative_path.to_string_lossy().to_string())
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

fn begin_transcription_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    meeting_id: &str,
    started_at_ms: u64,
) -> Result<CommandJobView, String> {
    let job = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state
            .begin_transcription_job(meeting_id, started_at_ms)
            .map_err(|job| {
                format!(
                    "{} already owns transcription for {}",
                    job.id, job.meeting_id
                )
            })?
    };

    let store = match open_store(app_root) {
        Ok(store) => store,
        Err(error) => {
            let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
            command_state.finish_transcription_job(&job, CommandJobFinishState::Failed);
            return Err(error);
        }
    };
    let active_job = match store.active_transcription_job_for_meeting(meeting_id) {
        Ok(active_job) => active_job,
        Err(error) => {
            let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
            command_state.finish_transcription_job(&job, CommandJobFinishState::Failed);
            return Err(error.to_string());
        }
    };
    if let Some(active_job) = active_job {
        let recovered_job = match store
            .recover_processing_job(
                &active_job.id,
                started_at_ms,
                "transcription worker was not running after app restart",
            )
            .and_then(|_| store.processing_job(&active_job.id))
        {
            Ok(recovered_job) => recovered_job,
            Err(error) => {
                let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
                command_state.finish_transcription_job(&job, CommandJobFinishState::Failed);
                return Err(error.to_string());
            }
        };
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.transcription_job = Some(command_job_from_processing_job(recovered_job));
        return Err(format!(
            "{} already owns transcription for {}",
            active_job.id, active_job.meeting_id
        ));
    }

    let durable_job = processing_job_from_command_job(&job);
    if let Err(error) = store.insert_processing_job(&durable_job) {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.finish_transcription_job(&job, CommandJobFinishState::Failed);
        return Err(error.to_string());
    }

    Ok(job)
}

fn processing_job_from_command_job(job: &CommandJobView) -> ProcessingJob {
    let kind = match job.kind {
        CommandJobKind::Transcription => JobKind::Transcribe,
        CommandJobKind::Summary => JobKind::Summarize,
    };
    let mut processing_job = ProcessingJob::new(&job.id, &job.meeting_id, kind, JobStatus::Running);
    processing_job.attempts = 1;
    processing_job.started_at_ms = Some(job.started_at_ms);
    processing_job.idempotency_key = Some(command_job_idempotency_key(job.kind, &job.meeting_id));
    processing_job
}

#[cfg(test)]
fn transcription_idempotency_key(meeting_id: &str) -> String {
    command_job_idempotency_key(CommandJobKind::Transcription, meeting_id)
}

#[cfg(test)]
fn summary_idempotency_key(meeting_id: &str) -> String {
    command_job_idempotency_key(CommandJobKind::Summary, meeting_id)
}

fn command_job_idempotency_key(kind: CommandJobKind, meeting_id: &str) -> String {
    match kind {
        CommandJobKind::Transcription => format!("transcribe:{meeting_id}"),
        CommandJobKind::Summary => format!("summarize:{meeting_id}"),
    }
}

fn command_job_from_processing_job(job: ProcessingJob) -> CommandJobView {
    let kind = match job.kind {
        JobKind::Transcribe => CommandJobKind::Transcription,
        JobKind::Summarize => CommandJobKind::Summary,
        JobKind::Export | JobKind::Index => {
            panic!("unsupported durable command job kind: {:?}", job.kind)
        }
    };
    CommandJobView {
        id: job.id,
        kind,
        meeting_id: job.meeting_id,
        state: command_job_state_from_processing_status(job.status, job.cancel_requested),
        cancel_requested: job.cancel_requested,
        started_at_ms: job.started_at_ms.unwrap_or_default(),
        last_error: job.last_error,
    }
}

fn command_job_state_from_processing_status(
    status: JobStatus,
    cancel_requested: bool,
) -> CommandJobState {
    match status {
        JobStatus::Running if cancel_requested => CommandJobState::CancelRequested,
        JobStatus::Running => CommandJobState::Running,
        JobStatus::Succeeded => CommandJobState::Complete,
        JobStatus::Canceled => CommandJobState::Canceled,
        JobStatus::Recovery => CommandJobState::Recovery,
        JobStatus::Retry => CommandJobState::Retry,
        JobStatus::Queued | JobStatus::Failed => CommandJobState::Failed,
    }
}

fn persist_transcription_job_cancel_request(app_root: &Path, job_id: &str) -> Result<(), String> {
    open_store(app_root)?
        .request_processing_job_cancel(job_id)
        .map_err(|error| error.to_string())
}

fn persist_summary_job_cancel_request(app_root: &Path, job_id: &str) -> Result<(), String> {
    open_store(app_root)?
        .request_processing_job_cancel(job_id)
        .map_err(|error| error.to_string())
}

fn persist_transcription_job_finish(
    app_root: &Path,
    job_id: &str,
    finish_state: CommandJobFinishState,
    finished_at_ms: u64,
    last_error: Option<&str>,
) -> Result<(), String> {
    let store = open_store(app_root)?;
    match finish_state {
        CommandJobFinishState::Complete => store.complete_processing_job(job_id, finished_at_ms),
        CommandJobFinishState::Canceled => store.cancel_processing_job(job_id, finished_at_ms),
        CommandJobFinishState::Failed => {
            store.fail_processing_job(job_id, finished_at_ms, last_error.unwrap_or("failed"))
        }
    }
    .map_err(|error| error.to_string())
}

fn persist_summary_job_finish(
    app_root: &Path,
    job_id: &str,
    finish_state: CommandJobFinishState,
    finished_at_ms: u64,
    last_error: Option<&str>,
) -> Result<(), String> {
    let store = open_store(app_root)?;
    match finish_state {
        CommandJobFinishState::Complete => store.complete_processing_job(job_id, finished_at_ms),
        CommandJobFinishState::Canceled => store.cancel_processing_job(job_id, finished_at_ms),
        CommandJobFinishState::Failed => {
            store.fail_processing_job(job_id, finished_at_ms, last_error.unwrap_or("failed"))
        }
    }
    .map_err(|error| error.to_string())
}

fn analysis_command_last_error(command: &AnalysisCommandView) -> Option<String> {
    command
        .failure
        .as_ref()
        .map(|failure| failure.message.clone())
}

fn finish_state_for_summary(
    command: &Result<Option<AnalysisCommandView>, String>,
) -> (CommandJobFinishState, Option<String>) {
    match command {
        Ok(Some(command)) if command.state == AnalysisCommandState::Failed => (
            CommandJobFinishState::Failed,
            analysis_command_last_error(command),
        ),
        Ok(Some(_)) => (CommandJobFinishState::Complete, None),
        Ok(None) => (CommandJobFinishState::Canceled, None),
        Err(error) => (CommandJobFinishState::Failed, Some(error.clone())),
    }
}

fn transcription_command_last_error(transcription: &TranscriptionCommandView) -> Option<String> {
    transcription
        .failure
        .as_ref()
        .map(|failure| failure.message.clone())
}

fn finish_state_for_transcription(
    transcription: &Result<Option<TranscriptionCommandView>, String>,
) -> (CommandJobFinishState, Option<String>) {
    match transcription {
        Ok(Some(transcription)) if transcription.state == TranscriptionCommandState::Failed => (
            CommandJobFinishState::Failed,
            transcription_command_last_error(transcription),
        ),
        Ok(Some(_)) => (CommandJobFinishState::Complete, None),
        Ok(None) => (CommandJobFinishState::Canceled, None),
        Err(error) => (CommandJobFinishState::Failed, Some(error.clone())),
    }
}

fn start_transcription_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    meeting_id: &str,
    started_at_ms: u64,
) -> Result<(CommandJobView, DesktopSnapshot), String> {
    let job =
        begin_transcription_job_for_app_root(app_root, command_state, meeting_id, started_at_ms)?;
    let snapshot_state = {
        let command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.snapshot_state()
    };
    let snapshot = match desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
            command_state.finish_transcription_job(&job, CommandJobFinishState::Failed);
            let _ = persist_transcription_job_finish(
                app_root,
                &job.id,
                CommandJobFinishState::Failed,
                started_at_ms,
                Some(&error),
            );
            return Err(error);
        }
    };
    Ok((job, snapshot))
}

fn cancel_transcription_job_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    job_id: &str,
) -> Result<DesktopSnapshot, String> {
    let snapshot_state = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.request_transcription_cancel(job_id)?;
        command_state.snapshot_state()
    };
    persist_transcription_job_cancel_request(app_root, job_id)?;
    desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state)
}

fn transcription_readiness_failure_snapshot_for_app_root(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    meeting_id: &str,
    settings: &AppSettings,
) -> Result<Option<DesktopSnapshot>, String> {
    let Some(failure) = whisper_transcription_readiness_failure(meeting_id, settings) else {
        return Ok(None);
    };
    let snapshot_state = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        command_state.last_transcription = Some(failure);
        command_state.snapshot_state()
    };
    desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state).map(Some)
}

fn whisper_transcription_readiness_failure(
    meeting_id: &str,
    settings: &AppSettings,
) -> Option<TranscriptionCommandView> {
    let configured_path = resolved_whisper_model_path(settings);
    let trimmed_path = configured_path.trim();
    if trimmed_path.is_empty() {
        return Some(transcription_failed(
            meeting_id,
            "missing_model",
            "No Whisper model path is configured.",
            "Choose a local Whisper model file, save it, run Test path, then retry transcription.",
        ));
    }
    if !PathBuf::from(trimmed_path).is_file() {
        return Some(transcription_failed(
            meeting_id,
            "missing_model",
            "Saved Whisper model path does not point to a readable model file.",
            "Choose a readable local Whisper model file, save it, run Test path, then retry transcription.",
        ));
    }
    if !is_supported_whisper_model_file_path(Path::new(trimmed_path)) {
        return Some(transcription_failed(
            meeting_id,
            "unsupported_model_file",
            "Saved Whisper model path must use a supported .bin or .gguf file.",
            "Choose an existing whisper.cpp-compatible .bin or .gguf Whisper model file, save it, then retry transcription.",
        ));
    }
    if !whisper_path_test_evidence_proves_current_readiness(
        trimmed_path,
        &settings.whisper_path_test_evidence,
    ) {
        return Some(transcription_failed(
            meeting_id,
            "model_path_untested",
            "Whisper model path needs matching Test path evidence before transcription.",
            "Run Test path for the saved Whisper model file, then retry transcription. Readability does not prove model compatibility.",
        ));
    }
    None
}

#[expect(
    clippy::too_many_arguments,
    reason = "job completion keeps state ownership, job identity, and backend inputs explicit"
)]
fn finish_transcription_job_for_app_root<B: WhisperBackend>(
    app_root: &Path,
    command_state: &Mutex<DesktopCommandState>,
    job: CommandJobView,
    meeting_id: &str,
    model_path: impl Into<PathBuf>,
    model_name: impl Into<String>,
    backend: B,
    created_at_ms: u64,
) -> Result<DesktopSnapshot, String> {
    let transcription = transcribe_meeting_command_with_cancellation(
        app_root,
        meeting_id,
        model_path,
        model_name,
        backend,
        created_at_ms,
        || transcription_job_cancel_requested(command_state, &job.id),
    );
    let (finish_state, last_error) = finish_state_for_transcription(&transcription);
    let snapshot_state = {
        let mut command_state = command_state.lock().map_err(|error| error.to_string())?;
        match &transcription {
            Ok(Some(transcription)) => {
                command_state.finish_transcription_job(&job, finish_state);
                command_state.last_transcription = Some(transcription.clone());
            }
            Ok(None) => {
                command_state.finish_transcription_job(&job, finish_state);
            }
            Err(_) => {
                command_state.finish_transcription_job(&job, finish_state);
            }
        }
        command_state.snapshot_state()
    };
    persist_transcription_job_finish(
        app_root,
        &job.id,
        finish_state,
        created_at_ms,
        last_error.as_deref(),
    )?;
    transcription?;
    desktop_snapshot_for_app_root_with_state(app_root, &snapshot_state)
}

#[cfg(test)]
fn transcribe_meeting_command<B: WhisperBackend>(
    app_root: &Path,
    meeting_id: &str,
    model_path: impl Into<PathBuf>,
    model_name: impl Into<String>,
    backend: B,
    created_at_ms: u64,
) -> Result<TranscriptionCommandView, String> {
    transcribe_meeting_command_with_cancellation(
        app_root,
        meeting_id,
        model_path,
        model_name,
        backend,
        created_at_ms,
        || false,
    )
    .map(|transcription| transcription.expect("non-cancelable transcription cannot be canceled"))
}

fn transcribe_meeting_command_with_cancellation<B: WhisperBackend>(
    app_root: &Path,
    meeting_id: &str,
    model_path: impl Into<PathBuf>,
    model_name: impl Into<String>,
    backend: B,
    created_at_ms: u64,
    is_cancelled: impl Fn() -> bool,
) -> Result<Option<TranscriptionCommandView>, String> {
    let model_path = model_path.into();
    let model_name = model_name.into();
    if is_cancelled() {
        return Ok(None);
    }
    let store = open_store(app_root)?;
    let artifacts = store
        .completed_wav_artifacts_for_transcription(meeting_id)
        .map_err(|error| error.to_string())?;
    if is_cancelled() {
        return Ok(None);
    }
    if artifacts.is_empty() {
        return Ok(Some(transcription_failed(
            meeting_id,
            "missing_audio_artifact",
            "No completed retained local WAV artifact exists for this meeting.",
            "Stop a desktop recording before requesting transcription.",
        )));
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
    let model_path_for_evidence = model_path.clone();
    let transcriber = WhisperTranscriber::new(model_path, model_name, backend);
    match transcriber.transcribe_wav_bundle(&requests) {
        Ok(document) => {
            if is_cancelled() {
                return Ok(None);
            }
            let compatibility_evidence = whisper_transcription_compatibility_evidence(
                &store,
                meeting_id,
                &model_path_for_evidence,
                &document,
                created_at_ms,
            );
            match persist_transcription_document(&store, meeting_id, document, created_at_ms) {
                Ok(()) => {
                    if let Some(evidence) = compatibility_evidence {
                        if let Err(error) =
                            store.save_whisper_transcription_compatibility_evidence(&evidence)
                        {
                            eprintln!(
                                "failed to persist Whisper transcription compatibility evidence: {error}"
                            );
                        }
                    }
                    cleanup_raw_audio_retention_after_transcription(
                        &store, meeting_id, &artifacts,
                    )?;
                    Ok(Some(TranscriptionCommandView {
                        meeting_id: meeting_id.to_string(),
                        state: TranscriptionCommandState::Complete,
                        failure: None,
                    }))
                }
                Err(error) => Ok(Some(transcription_failed(
                    meeting_id,
                    "transcript_persist_failed",
                    &format!("Transcription completed but could not be saved: {error}"),
                    "Check local app storage and retry transcription.",
                ))),
            }
        }
        Err(error) => {
            if is_cancelled() {
                Ok(None)
            } else {
                Ok(Some(transcription_failure_from_error(meeting_id, error)))
            }
        }
    }
}

fn whisper_transcription_compatibility_evidence(
    store: &Store,
    meeting_id: &str,
    model_path: &Path,
    document: &TranscriptionDocument,
    used_at_ms: u64,
) -> Option<WhisperTranscriptionCompatibilityEvidence> {
    if document.segments.is_empty() {
        return None;
    }
    let settings = store.app_settings().ok()?;
    let configured_path = resolved_whisper_model_path(&settings);
    let model_path = model_path.to_string_lossy().to_string();
    if model_path.trim() != configured_path.trim() {
        return None;
    }
    let metadata = std::fs::metadata(model_path.trim()).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified_at_ms = file_modified_at_ms(&metadata)?;
    Some(WhisperTranscriptionCompatibilityEvidence {
        model_path,
        used_at_ms,
        provider: document.provider.clone(),
        model_name: document.model_name.clone(),
        meeting_id: meeting_id.to_string(),
        model_run_id: document.model_run_id.clone(),
        transcript_version_id: document.transcript_version_id.clone(),
        segment_count: document.segments.len() as u64,
        file_size_bytes: metadata.len(),
        modified_at_ms,
    })
}

fn file_modified_at_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    u64::try_from(
        metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis(),
    )
    .ok()
}

fn cleanup_raw_audio_retention_after_transcription(
    store: &Store,
    meeting_id: &str,
    artifacts: &[curiosity_store::TranscriptionAudioArtifact],
) -> Result<(), String> {
    let artifact_ids = artifacts
        .iter()
        .map(|artifact| artifact.artifact_id.as_str())
        .collect::<Vec<_>>();
    let report = store
        .cleanup_raw_audio_artifacts_after_transcription(meeting_id, &artifact_ids)
        .map_err(|error| format!("Raw audio retention cleanup failed: {error}"))?;
    if !report.skipped_private_artifacts.is_empty() {
        let skipped = report
            .skipped_private_artifacts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Raw audio retention cleanup failed: skipped unsafe or user-owned artifact path(s): {skipped}"
        ));
    }
    Ok(())
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
        return recording_dto_with_retention(
            &active.meeting_id,
            Some(active.recording_id.clone()),
            CommandRecordingState::Recording,
            AppPermissionState::Ready,
            microphone_storage_path(&active.meeting_id),
            active.raw_audio_retention_policy,
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

fn meetings_have_system_audio_transcript(meetings: &[MeetingView]) -> bool {
    meetings.iter().any(|meeting| {
        meeting
            .segments
            .iter()
            .any(|segment| segment.source_channel == "System")
    })
}

fn system_audio_capture_state(
    command_state: &DesktopCommandSnapshotState,
    has_system_audio_transcript: bool,
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
    if has_system_audio_transcript {
        return DesktopPermissionState::Ready;
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
    recording_dto_with_retention(
        meeting_id,
        recording_id,
        state,
        permission_state,
        storage_path,
        RawAudioRetentionPolicy::Retain,
        recovery_action,
    )
}

fn recording_dto_with_retention(
    meeting_id: &str,
    recording_id: Option<String>,
    state: CommandRecordingState,
    permission_state: AppPermissionState,
    storage_path: String,
    raw_audio_retention: RawAudioRetentionPolicy,
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
        raw_audio_retention,
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

fn imported_artifact_id(recording_id: &str) -> String {
    format!("artifact-{recording_id}-imported")
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

fn imported_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/imported.wav",
        microphone_storage_path(meeting_id)
    )
}

fn imported_temp_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/imported.wav.tmp",
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
    let path = PathBuf::from(configured_path.trim());
    let kind = if configured_path.trim().is_empty() || !path.is_file() {
        "missing"
    } else if !is_supported_whisper_model_file_path(&path) {
        "unsupported"
    } else if whisper_path_test_evidence_proves_current_readiness(
        &configured_path,
        &settings.whisper_path_test_evidence,
    ) {
        "ready"
    } else {
        "untested"
    };
    ModelStatus {
        kind: kind.to_string(),
        configured_path,
    }
}

fn setup_guidance_from_settings(settings: &AppSettings) -> FirstRunSetupGuidanceView {
    FirstRunSetupGuidanceView {
        whisper: whisper_setup_guidance_from_settings(settings),
        ollama: ollama_setup_guidance_from_settings(settings),
    }
}

fn model_setup_options() -> ModelSetupOptionsView {
    let candidates = recommended_analysis_model_presets()
        .iter()
        .filter(|preset| {
            preset.provider_kind == AnalysisProviderKind::OllamaLocal
                && !preset.network_used
                && !preset.requires_data_disclosure
        })
        .map(|preset| OllamaModelSetupCandidateView {
            id: preset.id.to_string(),
            display_name: preset.display_name.to_string(),
            model_tag: preset.model_tag.to_string(),
            pull_command: format!("ollama pull {}", preset.model_tag),
            default_candidate: preset.default_candidate,
            setup_notes: preset.setup_notes.to_string(),
        })
        .collect();

    ModelSetupOptionsView {
        whisper: WhisperModelSetupOptionsView {
            mode: "ManualFile".to_string(),
            title: "Local Whisper file".to_string(),
            detail: "Choose an existing whisper.cpp-compatible .bin or .gguf model file. Curiosity does not download Whisper models yet."
                .to_string(),
            choose_label: "Choose model".to_string(),
            save_label: "Save Whisper".to_string(),
            test_label: "Test path".to_string(),
            downloads_managed: false,
            accepted_extensions: SUPPORTED_WHISPER_MODEL_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
        },
        ollama: OllamaModelSetupOptionsView {
            mode: "ManualOllama".to_string(),
            title: "Local Ollama models".to_string(),
            detail: "Start Ollama locally and install one of the listed local model tags manually before running Test Ollama."
                .to_string(),
            automatic_pulls: false,
            candidates,
        },
    }
}

fn calendar_context_snapshot(
    authorization_status: Option<AppleCalendarAuthorizationStatus>,
) -> CalendarContextView {
    let status = authorization_status.unwrap_or_else(apple_calendar_authorization_status);
    calendar_context_from_authorization(status)
}

#[cfg(test)]
fn request_apple_calendar_access_context() -> CalendarContextView {
    calendar_context_from_authorization(request_apple_calendar_full_access())
}

const APPLE_CALENDAR_LOOKAHEAD_HOURS: u64 = 24;
const APPLE_CALENDAR_MAX_EVENTS: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppleCalendarAuthorizationStatus {
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
struct CalendarContextEventDraft {
    event: CalendarContextEventView,
    has_stable_identifier: bool,
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

fn finalize_calendar_context_events(
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
fn request_apple_calendar_full_access() -> AppleCalendarAuthorizationStatus {
    AppleCalendarAuthorizationStatus::FullAccess
}

#[cfg(all(target_os = "macos", not(test)))]
fn request_apple_calendar_full_access() -> AppleCalendarAuthorizationStatus {
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
fn request_apple_calendar_full_access() -> AppleCalendarAuthorizationStatus {
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

fn whisper_setup_guidance_from_settings(settings: &AppSettings) -> WhisperSetupGuidanceView {
    let configured_path = resolved_whisper_model_path(settings);
    let last_path_test = matching_whisper_path_test_evidence(settings, &configured_path);
    let last_successful_transcription =
        matching_whisper_transcription_compatibility_evidence(settings, &configured_path);
    if configured_path.trim().is_empty() {
        return WhisperSetupGuidanceView {
            state: "MissingPath".to_string(),
            configured_path,
            message: "No Whisper model path is configured.".to_string(),
            setup_guidance:
                "Enter a local Whisper model path in Settings, save it, then use Test path."
                    .to_string(),
            compatibility_note: "Readability does not prove model compatibility.".to_string(),
            last_path_test,
            last_successful_transcription,
        };
    }

    let path = PathBuf::from(configured_path.trim());
    let unreadable = |message: String, setup_guidance: &str| WhisperSetupGuidanceView {
        state: "UnreadablePath".to_string(),
        configured_path: configured_path.clone(),
        message,
        setup_guidance: setup_guidance.to_string(),
        compatibility_note: "Readability does not prove model compatibility.".to_string(),
        last_path_test: last_path_test.clone(),
        last_successful_transcription: last_successful_transcription.clone(),
    };

    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return unreadable(
                format!("Whisper model path does not exist or cannot be inspected: {error}"),
                "Check the saved path, choose a readable local Whisper model file, then use Test path.",
            );
        }
    };
    if !metadata.is_file() {
        return unreadable(
            "Whisper model path must point to a file.".to_string(),
            "Choose a readable local Whisper model file, not a directory, then use Test path.",
        );
    }
    if !is_supported_whisper_model_file_path(&path) {
        return WhisperSetupGuidanceView {
            state: "UnsupportedFile".to_string(),
            configured_path,
            message: "Whisper model path must use a supported .bin or .gguf file.".to_string(),
            setup_guidance:
                "Choose an existing whisper.cpp-compatible .bin or .gguf Whisper model file."
                    .to_string(),
            compatibility_note: "Test path only accepts .bin and .gguf model files.".to_string(),
            last_path_test,
            last_successful_transcription,
        };
    }
    if let Err(error) = std::fs::File::open(&path) {
        return unreadable(
            format!("Whisper model path is not readable: {error}"),
            "Check file permissions, choose a readable local Whisper model file, then use Test path.",
        );
    }

    let (message, compatibility_note) = if last_successful_transcription.is_some() {
        (
            "Whisper model path is readable and has completed transcription before.".to_string(),
            "Last successful transcription is historical evidence for this local path, not a background compatibility check."
                .to_string(),
        )
    } else {
        (
            "Whisper model path is readable; compatibility is not verified.".to_string(),
            "Readability does not prove model compatibility.".to_string(),
        )
    };

    WhisperSetupGuidanceView {
        state: "ReadablePath".to_string(),
        configured_path,
        message,
        setup_guidance:
            "Use Test path for file evidence, then transcribe a sample to verify compatibility."
                .to_string(),
        compatibility_note,
        last_path_test,
        last_successful_transcription,
    }
}

fn ollama_setup_guidance_from_settings(settings: &AppSettings) -> OllamaSetupGuidanceView {
    let base_url = settings.ollama_base_url.trim().to_string();
    let model = canonical_local_ollama_model_tag(&settings.ollama_model);
    let last_connection_test = matching_ollama_connection_test_evidence(settings);
    let validation_error = validate_local_ollama_model(&model)
        .and_then(|_| local_ollama_endpoint(&base_url, "/api/tags").map(|_| ()))
        .err();

    let (state, availability, message, setup_guidance) = if let Some(error) = validation_error {
        if let Some(evidence) = last_connection_test
            .as_ref()
            .filter(|evidence| evidence.state == "Unavailable")
        {
            ollama_invalid_setup_guidance_from_last_test(error.to_string(), evidence)
        } else {
            (
                "InvalidLocalConfiguration",
                "UnknownUntilTest",
                error.to_string(),
                "Use a localhost or loopback Ollama URL and a local model tag, save it, then run Test Ollama. Availability is unknown until Test Ollama runs."
                    .to_string(),
            )
        }
    } else if let Some(evidence) = last_connection_test.as_ref() {
        ollama_setup_guidance_from_last_test(&model, evidence)
    } else {
        (
            "ConfiguredNotChecked",
            "UnknownUntilTest",
            "Ollama is configured for a local loopback URL and model.".to_string(),
            "Start Ollama manually, install the selected local model if needed, then run Test Ollama. Availability is unknown until Test Ollama runs."
                .to_string(),
        )
    };

    OllamaSetupGuidanceView {
        state: state.to_string(),
        base_url,
        model,
        availability: availability.to_string(),
        message,
        setup_guidance,
        last_connection_test,
    }
}

fn ollama_invalid_setup_guidance_from_last_test(
    validation_error: String,
    evidence: &OllamaConnectionTestEvidence,
) -> (&'static str, &'static str, String, String) {
    let failure_detail = evidence
        .failure_detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty());
    let setup_guidance = if failure_detail == Some(validation_error.as_str()) {
        format!(
            "{validation_error} Use a localhost or loopback Ollama URL and a local model tag, save it, then run Test Ollama again. Availability is not checked in the background."
        )
    } else if let Some(failure_detail) = failure_detail {
        format!(
            "{validation_error} Last explicit Test Ollama reported: {failure_detail} Use a localhost or loopback Ollama URL and a local model tag, save it, then run Test Ollama again. Availability is not checked in the background."
        )
    } else {
        format!(
            "{validation_error} Use a localhost or loopback Ollama URL and a local model tag, save it, then run Test Ollama again. Availability is not checked in the background."
        )
    };

    (
        "InvalidLocalConfiguration",
        "UnavailableAtLastTest",
        "Saved Ollama configuration is invalid; last explicit Test Ollama could not confirm local summary availability."
            .to_string(),
        setup_guidance,
    )
}

fn ollama_setup_guidance_from_last_test(
    model: &str,
    evidence: &OllamaConnectionTestEvidence,
) -> (&'static str, &'static str, String, String) {
    if evidence.state == "Available" {
        let selected_model = evidence
            .selected_local_model_tag
            .as_deref()
            .filter(|tag| !tag.trim().is_empty())
            .unwrap_or(model);
        return (
            "ConfiguredNotChecked",
            "AvailableAtLastTest",
            format!(
                "Last explicit Test Ollama reached {selected_model}; summaries were available at that test."
            ),
            "Availability is not checked in the background. Run Test Ollama again after changing Ollama, models, or the base URL."
                .to_string(),
        );
    }

    if let Some(pull_command) = evidence
        .pull_command
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        return (
            "ConfiguredNotChecked",
            "MissingModelAtLastTest",
            format!(
                "Last explicit Test Ollama reached Ollama, but {model} was missing. Summaries are unavailable until the selected local model is installed."
            ),
            format!(
                "Run `{pull_command}`, then run Test Ollama again. Availability is not checked in the background."
            ),
        );
    }

    let failure_detail = evidence
        .failure_detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty())
        .unwrap_or("The last explicit Test Ollama could not validate local Ollama.");
    (
        "ConfiguredNotChecked",
        "UnavailableAtLastTest",
        "Last explicit Test Ollama could not confirm local summary availability.".to_string(),
        format!(
            "{failure_detail} Start Ollama with `ollama serve`, verify the local base URL, then run Test Ollama again. Availability is not checked in the background."
        ),
    )
}

fn matching_whisper_path_test_evidence(
    settings: &AppSettings,
    configured_path: &str,
) -> Option<WhisperPathTestEvidence> {
    let configured_path = configured_path.trim();
    settings
        .whisper_path_test_evidence
        .as_ref()
        .filter(|evidence| evidence.tested_path == configured_path)
        .filter(|evidence| {
            evidence.state != "Valid"
                || whisper_path_test_evidence_matches_current_file(configured_path, evidence)
        })
        .cloned()
}

fn matching_whisper_transcription_compatibility_evidence(
    settings: &AppSettings,
    configured_path: &str,
) -> Option<WhisperTranscriptionCompatibilityEvidence> {
    let configured_path = configured_path.trim();
    if !is_supported_whisper_model_file_path(Path::new(configured_path)) {
        return None;
    }
    settings
        .whisper_transcription_compatibility_evidence
        .as_ref()
        .filter(|evidence| evidence.model_path == configured_path)
        .filter(|evidence| {
            whisper_transcription_compatibility_evidence_matches_current_file(
                configured_path,
                evidence,
            )
        })
        .cloned()
}

fn whisper_path_test_evidence_proves_current_readiness(
    configured_path: &str,
    evidence: &Option<WhisperPathTestEvidence>,
) -> bool {
    let configured_path = configured_path.trim();
    evidence
        .as_ref()
        .filter(|evidence| evidence.tested_path == configured_path)
        .filter(|evidence| evidence.state == "Valid")
        .map(|evidence| whisper_path_test_evidence_matches_current_file(configured_path, evidence))
        .unwrap_or(false)
}

fn whisper_transcription_compatibility_evidence_matches_current_file(
    configured_path: &str,
    evidence: &WhisperTranscriptionCompatibilityEvidence,
) -> bool {
    std::fs::metadata(configured_path.trim())
        .map(|metadata| {
            metadata.is_file()
                && metadata.len() == evidence.file_size_bytes
                && file_modified_at_ms(&metadata) == Some(evidence.modified_at_ms)
        })
        .unwrap_or(false)
}

fn whisper_path_test_evidence_matches_current_file(
    configured_path: &str,
    evidence: &WhisperPathTestEvidence,
) -> bool {
    let path = PathBuf::from(configured_path.trim());
    if !is_supported_whisper_model_file_path(&path) {
        return false;
    }
    let Some(expected_size) = evidence.file_size_bytes else {
        return false;
    };
    std::fs::metadata(&path)
        .map(|metadata| metadata.is_file() && metadata.len() == expected_size)
        .unwrap_or(false)
}

fn matching_ollama_connection_test_evidence(
    settings: &AppSettings,
) -> Option<OllamaConnectionTestEvidence> {
    let base_url = settings.ollama_base_url.trim();
    let model = canonical_local_ollama_model_tag(&settings.ollama_model);
    settings
        .ollama_connection_test_evidence
        .as_ref()
        .filter(|evidence| evidence.base_url == base_url && evidence.requested_model == model)
        .cloned()
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
        raw_audio_retention_policy: raw_audio_retention_policy_view(
            settings.raw_audio_retention_policy,
        ),
    }
}

fn raw_audio_retention_policy_view(
    policy: DomainRawAudioRetentionPolicy,
) -> RawAudioRetentionPolicy {
    match policy {
        DomainRawAudioRetentionPolicy::Retain => RawAudioRetentionPolicy::Retain,
        DomainRawAudioRetentionPolicy::DeleteAfterTranscription => {
            RawAudioRetentionPolicy::DeleteAfterTranscription
        }
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
    if !is_supported_whisper_model_file_path(&path) {
        return WhisperModelPathTestView::invalid(
            "Whisper model path must use a supported .bin or .gguf file.",
            "Choose an existing whisper.cpp-compatible .bin or .gguf model file, then run Test path.",
        );
    }
    match sha256_for_readable_file(&path) {
        Ok(sha256) => WhisperModelPathTestView {
            state: "Valid".to_string(),
            message: "Whisper model path is readable; compatibility is not verified by this test."
                .to_string(),
            setup_guidance:
                "Record this file size and SHA-256, then run the real Whisper smoke or transcribe a sample to verify compatibility."
                    .to_string(),
            file_size_bytes: Some(metadata.len()),
            sha256: Some(sha256),
        },
        Err(error) => WhisperModelPathTestView::invalid(
            format!("Whisper model path is not readable: {error}"),
            "Check file permissions and choose a readable local Whisper model file.",
        ),
    }
}

const SUPPORTED_WHISPER_MODEL_EXTENSIONS: [&str; 2] = ["bin", "gguf"];

fn is_supported_whisper_model_file_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_WHISPER_MODEL_EXTENSIONS
                .iter()
                .any(|supported_extension| extension.eq_ignore_ascii_case(supported_extension))
        })
        .unwrap_or(false)
}

fn sha256_for_readable_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
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
        let model_name = canonical_local_ollama_model_tag(model_name);
        validate_local_ollama_model(&model_name)?;
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
    let selected_model_tag = canonical_local_ollama_model_tag(model_name);
    let url = match local_ollama_endpoint(base_url, "/api/tags") {
        Ok(url) => url,
        Err(error) => {
            return OllamaConnectionTestView::unavailable(
                error.to_string(),
                "Use a local Ollama base URL such as http://127.0.0.1:11434.",
            )
            .with_selected_local_model_tag(selected_model_tag);
        }
    };
    let response = match transport.get_json(&url) {
        Ok(response) => response,
        Err(error) => {
            return OllamaConnectionTestView::unavailable(
                format!("Ollama is unavailable: {error}"),
                "Start Ollama with `ollama serve`, then retry.",
            )
            .with_selected_local_model_tag(selected_model_tag);
        }
    };
    let installed_models = installed_ollama_model_names(&response);
    let matched_model = installed_models
        .iter()
        .find(|installed_model| ollama_model_matches_request(installed_model, model_name))
        .cloned();
    if let Some(installed_model) = matched_model {
        OllamaConnectionTestView {
            state: "Available".to_string(),
            message: format!("Ollama is reachable and {installed_model} is installed."),
            setup_guidance: String::new(),
            selected_local_model_tag: Some(selected_model_tag),
            installed_local_models: Some(installed_models),
            pull_command: None,
        }
    } else {
        let pull_command = format!("ollama pull {selected_model_tag}");
        let installed_hint = if installed_models.is_empty() {
            " No local models were reported by Ollama.".to_string()
        } else {
            format!(" Installed local models: {}.", installed_models.join(", "))
        };
        let mut view = OllamaConnectionTestView::unavailable(
            format!("Ollama is reachable, but {selected_model_tag} is not installed."),
            format!(
                "Install the selected model with `{pull_command}`, then retry.{installed_hint}"
            ),
        )
        .with_selected_local_model_tag(selected_model_tag);
        view.installed_local_models = Some(installed_models);
        view.pull_command = Some(pull_command);
        view
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
    let installed_model = normalized_ollama_model_name(installed_model);
    requested_ollama_model_aliases(requested_model)
        .iter()
        .any(|alias| alias == &installed_model)
}

fn requested_ollama_model_aliases(requested_model: &str) -> Vec<String> {
    let trimmed = requested_model.trim();
    let mut aliases = Vec::new();
    push_unique_alias(&mut aliases, normalized_ollama_model_name(trimmed));
    if !trimmed.contains(':') {
        push_unique_alias(
            &mut aliases,
            normalized_ollama_model_name(&format!("{trimmed}:latest")),
        );
    }
    push_unique_alias(
        &mut aliases,
        normalized_ollama_model_name(&canonical_local_ollama_model_tag(trimmed)),
    );
    aliases
}

fn push_unique_alias(aliases: &mut Vec<String>, alias: String) {
    if !alias.is_empty() && !aliases.contains(&alias) {
        aliases.push(alias);
    }
}

fn canonical_local_ollama_model_tag(model_name: &str) -> String {
    let trimmed = model_name.trim();
    let normalized = normalized_ollama_model_name(trimmed);
    recommended_analysis_model_presets()
        .iter()
        .find(|preset| {
            preset.provider_kind == AnalysisProviderKind::OllamaLocal
                && (normalized_ollama_model_name(preset.model_tag) == normalized
                    || normalized_ollama_model_name(preset.id) == normalized
                    || normalized_ollama_model_name(preset.display_name) == normalized)
        })
        .map(|preset| preset.model_tag.to_string())
        .unwrap_or_else(|| trimmed.to_ascii_lowercase())
}

fn normalized_ollama_model_name(model_name: &str) -> String {
    model_name
        .trim()
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn validate_local_ollama_model(model_name: &str) -> Result<(), AnalysisClientError> {
    let trimmed = model_name.trim();
    if trimmed.is_empty() {
        return Err(AnalysisClientError::Transport(
            "Choose a local Ollama model before requesting analysis.".to_string(),
        ));
    }
    let normalized = normalized_ollama_model_name(trimmed);
    let is_hosted = normalized.ends_with(":cloud")
        || recommended_analysis_model_presets().iter().any(|preset| {
            preset.provider_kind != AnalysisProviderKind::OllamaLocal
                && (normalized_ollama_model_name(preset.model_tag) == normalized
                    || normalized_ollama_model_name(preset.id) == normalized
                    || normalized_ollama_model_name(preset.display_name) == normalized)
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
    setup_guidance: FirstRunSetupGuidanceView,
    model_setup_options: ModelSetupOptionsView,
    calendar_context: CalendarContextView,
    settings: AppSettingsView,
    capture: CaptureStatus,
    transcription: Option<TranscriptionCommandView>,
    transcription_job: Option<CommandJobView>,
    export_command: ExportCommandState,
    delete_command: DeleteCommandState,
    analysis_command: Option<AnalysisCommandView>,
    summary_job: Option<CommandJobView>,
}

#[derive(Clone, Debug, Serialize)]
struct CommandSurfaceState {
    ready: bool,
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
struct FirstRunSetupGuidanceView {
    whisper: WhisperSetupGuidanceView,
    ollama: OllamaSetupGuidanceView,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WhisperSetupGuidanceView {
    state: String,
    configured_path: String,
    message: String,
    setup_guidance: String,
    compatibility_note: String,
    last_path_test: Option<WhisperPathTestEvidence>,
    last_successful_transcription: Option<WhisperTranscriptionCompatibilityEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaSetupGuidanceView {
    state: String,
    base_url: String,
    model: String,
    availability: String,
    message: String,
    setup_guidance: String,
    last_connection_test: Option<OllamaConnectionTestEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelSetupOptionsView {
    whisper: WhisperModelSetupOptionsView,
    ollama: OllamaModelSetupOptionsView,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WhisperModelSetupOptionsView {
    mode: String,
    title: String,
    detail: String,
    choose_label: String,
    save_label: String,
    test_label: String,
    downloads_managed: bool,
    accepted_extensions: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaModelSetupOptionsView {
    mode: String,
    title: String,
    detail: String,
    automatic_pulls: bool,
    candidates: Vec<OllamaModelSetupCandidateView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaModelSetupCandidateView {
    id: String,
    display_name: String,
    model_tag: String,
    pull_command: String,
    default_candidate: bool,
    setup_notes: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CalendarContextView {
    source: String,
    permission_state: String,
    availability_state: String,
    message: String,
    setup_guidance: String,
    upcoming_events: Vec<CalendarContextEventView>,
    auto_start_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CalendarContextEventView {
    id: String,
    title: String,
    calendar_title: String,
    starts_at_ms: u64,
    ends_at_ms: u64,
    is_all_day: bool,
    is_recurring: bool,
    privacy: String,
    overlap_state: String,
    attachable: bool,
    safety_note: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSettingsView {
    whisper_model_path: String,
    ollama_base_url: String,
    ollama_model: String,
    export_directory: Option<String>,
    raw_audio_retention_policy: RawAudioRetentionPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WhisperModelPathTestView {
    state: String,
    message: String,
    setup_guidance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

impl WhisperModelPathTestView {
    fn invalid(message: impl Into<String>, setup_guidance: impl Into<String>) -> Self {
        Self {
            state: "Invalid".to_string(),
            message: message.into(),
            setup_guidance: setup_guidance.into(),
            file_size_bytes: None,
            sha256: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaConnectionTestView {
    state: String,
    message: String,
    setup_guidance: String,
    selected_local_model_tag: Option<String>,
    installed_local_models: Option<Vec<String>>,
    pull_command: Option<String>,
}

impl OllamaConnectionTestView {
    fn unavailable(message: impl Into<String>, setup_guidance: impl Into<String>) -> Self {
        Self {
            state: "Unavailable".to_string(),
            message: message.into(),
            setup_guidance: setup_guidance.into(),
            selected_local_model_tag: None,
            installed_local_models: None,
            pull_command: None,
        }
    }

    fn with_selected_local_model_tag(mut self, model_tag: String) -> Self {
        self.selected_local_model_tag = Some(model_tag);
        self
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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
    calendar_attachment: Option<MeetingCalendarAttachmentView>,
    analysis: Option<AnalysisDisclosureState>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeetingCalendarAttachmentView {
    source: String,
    event_id: String,
    event_title: String,
    calendar_title: String,
    starts_at_ms: u64,
    ends_at_ms: u64,
    privacy: String,
    privacy_confirmed: bool,
    attached_at_ms: u64,
}

impl MeetingCalendarAttachmentView {
    fn from_store(context: curiosity_store::MeetingCalendarContext) -> Self {
        Self {
            source: context.source,
            event_id: context.event_id,
            event_title: context.event_title,
            calendar_title: context.calendar_title,
            starts_at_ms: context.starts_at_ms,
            ends_at_ms: context.ends_at_ms,
            privacy: context.privacy,
            privacy_confirmed: context.privacy_confirmed,
            attached_at_ms: context.attached_at_ms,
        }
    }
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
    original_text: Option<String>,
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
    format: Option<ExportFormat>,
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
            format: None,
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
            format: Some(exported.format),
            path: Some(exported.path),
            message: None,
        }
    }

    fn failed(meeting_id: &str, format: ExportFormat, message: String) -> Self {
        Self {
            state: "failed".to_string(),
            meeting_id: Some(meeting_id.to_string()),
            format: Some(format),
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

fn delete_command_state_from_pending_finalization(
    report: PendingDeleteFinalizationReport,
) -> DeleteCommandState {
    DeleteCommandState::deleted(DeletedMeetingDto {
        meeting_id: report.meeting_id,
        deleted_private_artifacts: paths_to_strings(report.deleted_private_artifacts),
        skipped_private_artifacts: paths_to_strings(report.skipped_private_artifacts),
        remaining_exports: paths_to_strings(report.exported_files_outside_app_control),
    })
}

fn paths_to_strings(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect()
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
        RawAudioRetentionPolicy as DomainRawAudioRetentionPolicy, RecordingSession,
        RecordingSource, SourceChannel, TranscriptSegment, TranscriptVersion,
    };
    use curiosity_transcription::{FakeWhisperBackend, WhisperBackendSegment};
    use sha2::{Digest, Sha256};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
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
        assert_eq!(json["commandSurface"]["ready"], true);
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
        assert_eq!(json["calendarContext"]["source"], "AppleCalendar");
        assert_eq!(json["calendarContext"]["permissionState"], "NotRequested");
        assert_eq!(
            json["calendarContext"]["availabilityState"],
            "PermissionRequired"
        );
        assert_eq!(json["calendarContext"]["autoStartEnabled"], false);
        assert_eq!(
            json["calendarContext"]["upcomingEvents"]
                .as_array()
                .expect("upcoming calendar events")
                .len(),
            0
        );
        assert_eq!(json["settings"]["ollamaBaseUrl"], "http://127.0.0.1:11434");
        assert_eq!(json["settings"]["ollamaModel"], "qwen3.6:27b");
        assert!(json["transcription"].is_null());

        restore_whisper_env(previous);
        fs::remove_dir_all(root).expect("cleanup");
    }

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
    fn attach_calendar_event_context_persists_backend_resolved_event() {
        let root = unique_test_root();
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
            .expect("insert meeting");
        drop(store);

        let mut unknown_privacy = calendar_event_draft("event-1", "Design Review", 2_000, 3_000);
        unknown_privacy.event.privacy = "Unknown".to_string();
        let events = finalize_calendar_context_events(vec![unknown_privacy]);
        let snapshot_state = DesktopCommandSnapshotState::default();
        let snapshot = attach_calendar_event_context_for_app_root_with_events(
            &root,
            &snapshot_state,
            "meeting-1",
            "event-1",
            true,
            4_000,
            events,
        )
        .expect("attach calendar event");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            json["meetings"][0]["calendarAttachment"]["eventId"],
            "event-1"
        );
        assert_eq!(
            json["meetings"][0]["calendarAttachment"]["eventTitle"],
            "Design Review"
        );
        assert_eq!(
            json["meetings"][0]["calendarAttachment"]["privacyConfirmed"],
            true
        );
    }

    #[test]
    fn attach_calendar_event_context_rejects_unavailable_or_unsafe_events() {
        let root = unique_test_root();
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
            .expect("insert meeting");
        drop(store);

        let snapshot_state = DesktopCommandSnapshotState::default();
        let missing_error = attach_calendar_event_context_for_app_root_with_events(
            &root,
            &snapshot_state,
            "meeting-1",
            "missing-event",
            true,
            4_000,
            Vec::new(),
        )
        .expect_err("missing backend event should reject");
        assert!(missing_error.contains("no longer available"));

        let mut recurring = calendar_event_draft("event-1", "Recurring", 2_000, 3_000);
        recurring.event.is_recurring = true;
        let unsafe_events = finalize_calendar_context_events(vec![recurring]);
        let unsafe_error = attach_calendar_event_context_for_app_root_with_events(
            &root,
            &snapshot_state,
            "meeting-1",
            "event-1",
            true,
            4_000,
            unsafe_events,
        )
        .expect_err("unsafe backend event should reject");
        assert!(unsafe_error.contains("not marked attachable"));
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

    fn calendar_event_draft(
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

    #[test]
    fn desktop_command_view_contract_fixture_matches_rust_serialization() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let _whisper_env = EnvVarRestoreGuard::unset("CURIOSITY_WHISPER_MODEL");

        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../contracts/desktop-command-view-contract.fixture.json");
        let fixture_text = fs::read_to_string(&fixture_path).unwrap_or_else(|error| {
            panic!(
                "read desktop command/view contract fixture at {}: {error}",
                fixture_path.display()
            )
        });
        let expected: serde_json::Value =
            serde_json::from_str(&fixture_text).expect("parse desktop command/view fixture");
        let actual = desktop_command_view_contract_fixture();
        if std::env::var_os("UPDATE_DESKTOP_COMMAND_VIEW_CONTRACT").is_some() {
            let fixture_json = serde_json::to_string_pretty(&actual)
                .expect("serialize desktop command/view fixture");
            fs::write(&fixture_path, format!("{fixture_json}\n")).unwrap_or_else(|error| {
                panic!(
                    "write desktop command/view contract fixture at {}: {error}",
                    fixture_path.display()
                )
            });
            return;
        }

        assert_eq!(
            actual, expected,
            "Rust desktop command/view serialization no longer matches {}. Update the fixture intentionally when DTOs change.",
            fixture_path.display()
        );
    }

    #[test]
    fn whisper_env_restore_guard_restores_value_during_unwind() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let _restore_original = EnvVarRestoreGuard::capture("CURIOSITY_WHISPER_MODEL");
        std::env::set_var("CURIOSITY_WHISPER_MODEL", "before-guard");

        let result = std::panic::catch_unwind(|| {
            let _restore = EnvVarRestoreGuard::unset("CURIOSITY_WHISPER_MODEL");
            assert!(std::env::var("CURIOSITY_WHISPER_MODEL").is_err());
            panic!("force env restore during unwind");
        });

        assert!(result.is_err());
        assert_eq!(
            std::env::var("CURIOSITY_WHISPER_MODEL").as_deref(),
            Ok("before-guard")
        );
    }

    #[test]
    fn get_settings_returns_default_local_analysis_settings() {
        let root = unique_test_root();

        let settings = get_settings_for_app_root(&root).expect("settings");

        assert_eq!(settings.whisper_model_path, "");
        assert_eq!(settings.ollama_base_url, "http://127.0.0.1:11434");
        assert_eq!(settings.ollama_model, "qwen3.6:27b");
        assert_eq!(settings.export_directory, None);
        assert_eq!(
            settings.raw_audio_retention_policy,
            RawAudioRetentionPolicy::Retain
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_settings_commands_persist_whisper_and_analysis_values() {
        let root = unique_test_root();

        save_whisper_model_path_for_app_root(&root, "/models/ggml-base.en.bin".to_string())
            .expect("save whisper");
        save_analysis_settings_for_app_root(
            &root,
            "http://localhost:11434".to_string(),
            "Qwen 3.6 27B".to_string(),
        )
        .expect("save qwen display name");
        let qwen_settings = get_settings_for_app_root(&root).expect("qwen settings");
        assert_eq!(qwen_settings.ollama_base_url, "http://localhost:11434");
        assert_eq!(qwen_settings.ollama_model, "qwen3.6:27b");

        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "ollama-qwen3-6-27b".to_string(),
        )
        .expect("save qwen preset id");
        let qwen_id_settings = get_settings_for_app_root(&root).expect("qwen id settings");
        assert_eq!(qwen_id_settings.ollama_model, "qwen3.6:27b");

        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11435".to_string(),
            "Gemma 4 31B".to_string(),
        )
        .expect("save analysis");
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save retention");
        let settings = get_settings_for_app_root(&root).expect("settings");

        assert_eq!(settings.whisper_model_path, "/models/ggml-base.en.bin");
        assert_eq!(settings.ollama_base_url, "http://127.0.0.1:11435");
        assert_eq!(settings.ollama_model, "gemma4:31b");
        assert_eq!(
            settings.raw_audio_retention_policy,
            RawAudioRetentionPolicy::DeleteAfterTranscription
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_setup_tests_persist_matching_snapshot_evidence() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let _whisper_env = EnvVarRestoreGuard::unset("CURIOSITY_WHISPER_MODEL");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let model_path = root.join("fixture-whisper.bin");
        let model_bytes = b"not a real model";
        fs::write(&model_path, model_bytes).expect("model file");
        let model_path = model_path.to_string_lossy().to_string();
        let expected_sha256 = format!("{:x}", Sha256::digest(model_bytes));

        let whisper_result =
            test_whisper_model_path_for_app_root(&root, model_path.clone(), 1_700_000_001_000)
                .expect("test whisper path");
        save_whisper_model_path_for_app_root(&root, model_path.clone()).expect("save whisper");
        let ollama_result = test_ollama_connection_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "Qwen3.6:27B".to_string(),
            &RecordingOllamaTransport::tags_response(
                r#"{"models":[{"name":"gemma4:31b"},{"name":"qwen3.6:27b"}]}"#,
            ),
            1_700_000_002_000,
        )
        .expect("test ollama");
        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "Qwen3.6:27B".to_string(),
        )
        .expect("save analysis");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(whisper_result.state, "Valid");
        assert_eq!(ollama_result.state, "Available");
        assert_eq!(json["model"]["kind"], "ready");
        assert_eq!(
            json["setupGuidance"]["ollama"]["availability"],
            "AvailableAtLastTest"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["message"],
            "Last explicit Test Ollama reached qwen3.6:27b; summaries were available at that test."
        );
        assert!(json["setupGuidance"]["ollama"]["setupGuidance"]
            .as_str()
            .expect("ollama setup guidance")
            .contains("Availability is not checked in the background"));
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"]["testedPath"],
            model_path
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"]["testedAtMs"],
            1_700_000_001_000_u64
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"]["fileSizeBytes"],
            model_bytes.len() as u64
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"]["sha256"],
            expected_sha256
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"]["failureDetail"],
            serde_json::Value::Null
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["baseUrl"],
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["requestedModel"],
            "qwen3.6:27b"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["testedAtMs"],
            1_700_000_002_000_u64
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["selectedLocalModelTag"],
            "qwen3.6:27b"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["installedLocalModels"],
            serde_json::json!(["gemma4:31b", "qwen3.6:27b"])
        );
        assert!(!json["setupGuidance"]["whisper"]
            .to_string()
            .to_lowercase()
            .contains("is compatible"));

        save_whisper_model_path_for_app_root(&root, "/models/stale.bin".to_string())
            .expect("save mismatched whisper");
        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11435".to_string(),
            "qwen3.6:27b".to_string(),
        )
        .expect("save mismatched analysis");
        let stale_snapshot = desktop_snapshot_for_app_root(&root).expect("stale snapshot");
        let stale_json = serde_json::to_value(&stale_snapshot).expect("serialize stale snapshot");
        assert_eq!(
            stale_json["setupGuidance"]["whisper"]["lastPathTest"],
            serde_json::Value::Null
        );
        assert_eq!(stale_json["model"]["kind"], "missing");
        assert_eq!(
            stale_json["setupGuidance"]["ollama"]["lastConnectionTest"],
            serde_json::Value::Null
        );
        assert_eq!(
            stale_json["setupGuidance"]["ollama"]["availability"],
            "UnknownUntilTest"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_marks_ollama_model_missing_from_matching_last_test_evidence() {
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let ollama_result = test_ollama_connection_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "Qwen3.6:27B".to_string(),
            &RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"gemma4:31b"}]}"#),
            1_700_000_003_000,
        )
        .expect("test ollama");
        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "qwen3.6:27b".to_string(),
        )
        .expect("save analysis");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(ollama_result.state, "Unavailable");
        assert_eq!(
            json["setupGuidance"]["ollama"]["availability"],
            "MissingModelAtLastTest"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["pullCommand"],
            "ollama pull qwen3.6:27b"
        );
        assert!(json["setupGuidance"]["ollama"]["message"]
            .as_str()
            .expect("ollama message")
            .contains("qwen3.6:27b was missing"));
        assert!(json["setupGuidance"]["ollama"]["setupGuidance"]
            .as_str()
            .expect("ollama setup guidance")
            .contains("Run `ollama pull qwen3.6:27b`, then run Test Ollama again"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_marks_summaries_unavailable_from_matching_failed_ollama_test_evidence() {
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let ollama_result = test_ollama_connection_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "qwen3.6:27b".to_string(),
            &RecordingOllamaTransport::tags_http_error(500, r#"{"error":"tags unavailable"}"#),
            1_700_000_004_000,
        )
        .expect("test ollama");
        save_analysis_settings_for_app_root(
            &root,
            "http://127.0.0.1:11434".to_string(),
            "qwen3.6:27b".to_string(),
        )
        .expect("save analysis");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(ollama_result.state, "Unavailable");
        assert_eq!(
            json["setupGuidance"]["ollama"]["availability"],
            "UnavailableAtLastTest"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"]["pullCommand"],
            serde_json::Value::Null
        );
        assert!(json["setupGuidance"]["ollama"]["setupGuidance"]
            .as_str()
            .expect("ollama setup guidance")
            .contains("HTTP 500"));
        assert!(json["setupGuidance"]["ollama"]["setupGuidance"]
            .as_str()
            .expect("ollama setup guidance")
            .contains("Availability is not checked in the background"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_analysis_settings_rejects_non_local_privacy_paths_without_mutating_settings() {
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        save_analysis_settings_for_app_root(
            &root,
            "http://localhost:11435".to_string(),
            "gemma4:31b".to_string(),
        )
        .expect("save baseline");

        for hosted_model in [
            "deepseek-v3.2:cloud",
            "ollama-cloud-deepseek-v3-2",
            "hosted-deepseek-v3-2-speciale",
            "DeepSeek V3.2 Speciale",
        ] {
            let error = save_analysis_settings_for_app_root(
                &root,
                "http://127.0.0.1:11434".to_string(),
                hosted_model.to_string(),
            )
            .expect_err("hosted model should not be saved as local analysis settings");
            assert!(
                error.contains("hosted or cloud model tags"),
                "unexpected error for {hosted_model}: {error}"
            );
        }

        for remote_base_url in ["https://ollama.example.com", "http://192.168.1.20:11434"] {
            let error = save_analysis_settings_for_app_root(
                &root,
                remote_base_url.to_string(),
                "qwen3.6:27b".to_string(),
            )
            .expect_err("non-loopback Ollama URL should not be saved");
            assert!(
                error.contains("loopback"),
                "unexpected error for {remote_base_url}: {error}"
            );
        }

        let settings = get_settings_for_app_root(&root).expect("settings after rejected saves");
        assert_eq!(settings.ollama_base_url, "http://localhost:11435");
        assert_eq!(settings.ollama_model, "gemma4:31b");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        assert_eq!(
            json["setupGuidance"]["ollama"]["state"],
            "ConfiguredNotChecked"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_setup_test_fails_loudly_when_evidence_cannot_be_persisted() {
        let root = unique_test_root();
        fs::write(&root, b"not a directory").expect("blocking file");

        let error = test_whisper_model_path_for_app_root(&root, "".to_string(), 1_700_000_001_000)
            .expect_err("persistence failure should fail the explicit test command");

        assert!(!error.trim().is_empty());
        let _ = fs::remove_file(root);
    }

    #[test]
    fn save_raw_audio_retention_setting_persists_supported_policy_and_rejects_never_save() {
        let root = unique_test_root();

        let settings = save_raw_audio_retention_policy_for_app_root(
            &root,
            "DeleteAfterTranscription".to_string(),
        )
        .expect("save delete-after retention");
        let error = save_raw_audio_retention_policy_for_app_root(&root, "NeverSave".to_string())
            .expect_err("NeverSave remains unsupported");
        let reopened = get_settings_for_app_root(&root).expect("settings after rejected save");

        assert_eq!(
            settings.raw_audio_retention_policy,
            RawAudioRetentionPolicy::DeleteAfterTranscription
        );
        assert!(
            error.contains("unsupported raw audio retention policy"),
            "unsupported policy should fail loudly: {error}"
        );
        assert_eq!(
            reopened.raw_audio_retention_policy,
            RawAudioRetentionPolicy::DeleteAfterTranscription
        );

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
    fn local_ollama_client_posts_canonical_local_preset_tags_for_case_variants() {
        let qwen_transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Local summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[]}"}"#,
        );
        let qwen_client =
            LocalOllamaTextClient::new("http://127.0.0.1:11434", qwen_transport.clone());

        qwen_client
            .complete("qwen3.6:27B", "summarize locally")
            .expect("uppercase size suffix should use the local preset tag");

        let qwen_request = qwen_transport
            .last_generate_request()
            .expect("qwen generate request");
        assert_eq!(qwen_request.body["model"], "qwen3.6:27b");

        let gemma_transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Local summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[]}"}"#,
        );
        let gemma_client =
            LocalOllamaTextClient::new("http://127.0.0.1:11434", gemma_transport.clone());

        gemma_client.complete("Gemma4", "summarize locally").expect(
            "tagless model family should normalize without forcing a non-installed size tag",
        );

        let gemma_request = gemma_transport
            .last_generate_request()
            .expect("gemma generate request");
        assert_eq!(gemma_request.body["model"], "gemma4");
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
        assert_eq!(
            result.selected_local_model_tag.as_deref(),
            Some("qwen3.6:27b")
        );
        assert_eq!(
            result.installed_local_models.as_deref(),
            Some(["gemma4:31b".to_string(), "qwen3.6:27b".to_string()].as_slice())
        );
        assert_eq!(result.pull_command, None);
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
    fn test_ollama_connection_matches_local_model_tags_without_case_sensitive_false_missing() {
        let transport = RecordingOllamaTransport::tags_response(
            r#"{"models":[{"name":"qwen3.6:27b"},{"name":"gemma4:31b"}]}"#,
        );

        let qwen =
            test_ollama_connection_value("http://127.0.0.1:11434", "qwen3.6:27B", &transport);

        assert_eq!(qwen.state, "Available");
        assert!(qwen.message.contains("qwen3.6:27b"));
    }

    #[test]
    fn test_ollama_connection_does_not_accept_family_alias_for_unverified_size_tag() {
        let transport =
            RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"gemma4:31b"}]}"#);

        let gemma = test_ollama_connection_value("http://127.0.0.1:11434", "Gemma4", &transport);

        assert_eq!(gemma.state, "Unavailable");
        assert!(gemma.message.contains("gemma4"));
        assert!(gemma.setup_guidance.contains("ollama pull gemma4"));
        assert!(gemma
            .setup_guidance
            .contains("Installed local models: gemma4:31b"));
    }

    #[test]
    fn test_ollama_connection_accepts_explicit_size_tag_with_different_casing() {
        let transport =
            RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"gemma4:31b"}]}"#);

        let gemma =
            test_ollama_connection_value("http://127.0.0.1:11434", "Gemma4:31B", &transport);

        assert_eq!(gemma.state, "Available");
        assert!(gemma.message.contains("gemma4:31b"));
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
        assert_eq!(
            result.selected_local_model_tag.as_deref(),
            Some("qwen3.6:27b")
        );
        assert_eq!(
            result.installed_local_models.as_deref(),
            Some(["gemma4:31b".to_string()].as_slice())
        );
        assert_eq!(
            result.pull_command.as_deref(),
            Some("ollama pull qwen3.6:27b")
        );
    }

    #[test]
    fn test_ollama_connection_rejects_cloud_model_without_local_setup_metadata() {
        let transport =
            RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"qwen3.6:27b"}]}"#);

        let result = test_ollama_connection_value(
            "http://127.0.0.1:11434",
            "deepseek-v3.2:cloud",
            &transport,
        );

        assert_eq!(result.state, "Unavailable");
        assert!(result.message.contains("hosted or cloud model tags"));
        assert_eq!(result.selected_local_model_tag, None);
        assert_eq!(result.installed_local_models, None);
        assert_eq!(result.pull_command, None);
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

        let _ = fs::remove_dir_all(root);
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
        let model_bytes = b"not a real model";
        fs::write(&model_path, model_bytes).expect("model file");
        let expected_sha256 = format!("{:x}", Sha256::digest(model_bytes));

        let result = test_whisper_model_path_value(model_path.to_string_lossy().as_ref());
        let json = serde_json::to_value(&result).expect("serialize result");

        assert_eq!(result.state, "Valid");
        assert!(result.message.contains("readable"));
        assert_eq!(
            json["fileSizeBytes"].as_u64(),
            Some(model_bytes.len() as u64)
        );
        assert_eq!(json["sha256"].as_str(), Some(expected_sha256.as_str()));
        let readiness_copy = format!("{} {}", result.message, result.setup_guidance).to_lowercase();
        assert!(readiness_copy.contains("compatibility"));
        assert!(!readiness_copy.contains("is compatible"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_whisper_model_path_accepts_readable_gguf_file_without_loading_model() {
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let model_path = root.join("fixture-whisper.gguf");
        fs::write(&model_path, b"not a real gguf model").expect("model file");

        let result = test_whisper_model_path_value(model_path.to_string_lossy().as_ref());

        assert_eq!(result.state, "Valid");
        assert!(result.file_size_bytes.is_some());
        assert!(result.sha256.is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_whisper_model_path_rejects_missing_path_with_guidance() {
        let result = test_whisper_model_path_value("");

        assert_eq!(result.state, "Invalid");
        assert!(result.setup_guidance.contains("local Whisper model path"));
    }

    #[test]
    fn test_whisper_model_path_rejects_unsupported_readable_files_without_readiness_evidence() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");

        for file_name in ["notes.txt", "extensionless"] {
            let unsupported_path = root.join(file_name);
            fs::write(
                &unsupported_path,
                b"readable but not a supported model file",
            )
            .expect("unsupported readable file");
            let unsupported_path = unsupported_path.to_string_lossy().to_string();

            save_whisper_model_path_for_app_root(&root, unsupported_path.clone())
                .expect("save whisper path");
            let result = test_whisper_model_path_for_app_root(
                &root,
                unsupported_path.clone(),
                1_700_000_001_000,
            )
            .expect("test unsupported whisper path");

            assert_eq!(result.state, "Invalid");
            assert!(result.message.contains(".bin") && result.message.contains(".gguf"));
            assert!(
                result.setup_guidance.contains(".bin") && result.setup_guidance.contains(".gguf")
            );

            let settings = app_settings_for_app_root(&root).expect("settings");
            assert_ne!(
                settings
                    .whisper_path_test_evidence
                    .as_ref()
                    .map(|evidence| evidence.state.as_str()),
                Some("Valid")
            );

            let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
            let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
            assert_ne!(json["model"]["kind"], "ready");
            assert_ne!(
                json["setupGuidance"]["whisper"]["lastPathTest"]["state"],
                "Valid"
            );
        }

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_snapshot_ignores_legacy_valid_evidence_for_unsupported_whisper_file() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");

        let unsupported_path = root.join("notes.txt");
        let unsupported_bytes = b"legacy readable file evidence";
        fs::write(&unsupported_path, unsupported_bytes).expect("unsupported readable file");
        let unsupported_path = unsupported_path.to_string_lossy().to_string();
        save_whisper_model_path_for_app_root(&root, unsupported_path.clone())
            .expect("save whisper path");
        open_store(&root)
            .expect("store")
            .save_whisper_path_test_evidence(&WhisperPathTestEvidence {
                tested_path: unsupported_path,
                tested_at_ms: 1_700_000_001_000,
                state: "Valid".to_string(),
                file_size_bytes: Some(unsupported_bytes.len() as u64),
                sha256: Some(format!("{:x}", Sha256::digest(unsupported_bytes))),
                failure_detail: None,
            })
            .expect("persist legacy valid evidence");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_ne!(json["model"]["kind"], "ready");
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"],
            serde_json::Value::Null
        );

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
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

        assert_eq!(json["model"]["kind"], "untested");
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
    fn desktop_snapshot_guides_missing_whisper_and_unchecked_ollama_without_probe_metadata() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["setupGuidance"]["whisper"]["state"], "MissingPath");
        assert_eq!(
            json["setupGuidance"]["whisper"]["message"],
            "No Whisper model path is configured."
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["compatibilityNote"],
            "Readability does not prove model compatibility."
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["availability"],
            "UnknownUntilTest"
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["baseUrl"],
            "http://127.0.0.1:11434"
        );
        assert_eq!(json["setupGuidance"]["ollama"]["model"], "qwen3.6:27b");
        let ollama = json["setupGuidance"]["ollama"]
            .as_object()
            .expect("ollama guidance");
        assert!(!ollama.contains_key("installedLocalModels"));
        assert!(!ollama.contains_key("pullCommand"));
        assert!(!json["setupGuidance"]["ollama"]
            .to_string()
            .contains("ollama pull"));
        assert!(!json["setupGuidance"]["ollama"]
            .to_string()
            .to_lowercase()
            .contains("download"));
        let whisper = json["setupGuidance"]["whisper"]
            .as_object()
            .expect("whisper guidance");
        let ollama = json["setupGuidance"]["ollama"]
            .as_object()
            .expect("ollama guidance");
        assert!(whisper.contains_key("lastPathTest"));
        assert!(ollama.contains_key("lastConnectionTest"));
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"],
            serde_json::Value::Null
        );
        assert_eq!(
            json["setupGuidance"]["ollama"]["lastConnectionTest"],
            serde_json::Value::Null
        );

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_snapshot_exposes_manual_model_setup_options_without_downloads_or_hosted_models() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let setup_options = &json["modelSetupOptions"];

        assert_eq!(
            setup_options["whisper"]["mode"],
            serde_json::json!("ManualFile")
        );
        assert_eq!(setup_options["whisper"]["downloadsManaged"], false);
        assert_eq!(
            setup_options["whisper"]["acceptedExtensions"],
            serde_json::json!(["bin", "gguf"])
        );
        assert!(setup_options["whisper"]["detail"]
            .as_str()
            .expect("whisper setup detail")
            .contains("does not download Whisper models yet"));

        assert_eq!(
            setup_options["ollama"]["mode"],
            serde_json::json!("ManualOllama")
        );
        assert_eq!(setup_options["ollama"]["automaticPulls"], false);
        let candidates = setup_options["ollama"]["candidates"]
            .as_array()
            .expect("ollama candidates");
        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .iter()
            .any(|candidate| candidate["modelTag"] == "qwen3.6:27b"
                && candidate["pullCommand"] == "ollama pull qwen3.6:27b"));
        assert!(candidates
            .iter()
            .any(|candidate| candidate["modelTag"] == "gemma4:31b"
                && candidate["pullCommand"] == "ollama pull gemma4:31b"));
        assert!(!setup_options["ollama"]
            .to_string()
            .contains("deepseek-v3.2:cloud"));

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_snapshot_guides_readable_whisper_as_unverified_without_hashing() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"not a real model").expect("model file");
        save_whisper_model_path_for_app_root(&root, model_path.to_string_lossy().to_string())
            .expect("save whisper");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["model"]["kind"], "untested");
        assert_eq!(json["setupGuidance"]["whisper"]["state"], "ReadablePath");
        assert_eq!(
            json["setupGuidance"]["whisper"]["configuredPath"],
            model_path.to_string_lossy().as_ref()
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["message"],
            "Whisper model path is readable; compatibility is not verified."
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["compatibilityNote"],
            "Readability does not prove model compatibility."
        );
        let whisper = json["setupGuidance"]["whisper"]
            .as_object()
            .expect("whisper guidance");
        assert!(!whisper.contains_key("sha256"));
        assert!(!whisper.contains_key("fileSizeBytes"));
        assert!(whisper.contains_key("lastPathTest"));
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"],
            serde_json::Value::Null
        );

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_snapshot_marks_changed_whisper_file_as_untested_without_rehashing() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"first version").expect("model file");
        let model_path = model_path.to_string_lossy().to_string();

        test_whisper_model_path_for_app_root(&root, model_path.clone(), 1_700_000_001_000)
            .expect("test whisper path");
        save_whisper_model_path_for_app_root(&root, model_path.clone()).expect("save whisper");
        fs::write(&model_path, b"changed file with a different size").expect("changed model file");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["model"]["kind"], "untested");
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"],
            serde_json::Value::Null
        );
        assert!(!json["setupGuidance"]["whisper"]
            .to_string()
            .contains("first version"));

        restore_whisper_env(previous);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_snapshot_guides_existing_directory_whisper_path_as_unreadable_without_hashing() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let previous = std::env::var("CURIOSITY_WHISPER_MODEL").ok();
        std::env::remove_var("CURIOSITY_WHISPER_MODEL");
        let model_directory = root.join("not-a-model-file");
        fs::create_dir_all(&model_directory).expect("model directory");
        save_whisper_model_path_for_app_root(&root, model_directory.to_string_lossy().to_string())
            .expect("save whisper");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["setupGuidance"]["whisper"]["state"], "UnreadablePath");
        assert_eq!(
            json["setupGuidance"]["whisper"]["message"],
            "Whisper model path must point to a file."
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["compatibilityNote"],
            "Readability does not prove model compatibility."
        );
        let whisper = json["setupGuidance"]["whisper"]
            .as_object()
            .expect("whisper guidance");
        assert!(!whisper.contains_key("sha256"));
        assert!(!whisper.contains_key("fileSizeBytes"));
        let readiness_copy = json["setupGuidance"]["whisper"].to_string().to_lowercase();
        assert!(!readiness_copy.contains("is compatible"));
        assert!(!readiness_copy.contains("compatibility is verified"));

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
        let command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Original Planning",
            "rename target",
        );

        let snapshot = rename_meeting_for_app_root(
            &root,
            &command_state.snapshot_state(),
            "meeting-1",
            "Renamed Planning",
        )
        .expect("rename meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["selectedMeetingId"], "meeting-1");
        assert_eq!(json["meetings"][0]["title"], "Renamed Planning");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn correct_transcript_segment_command_refreshes_snapshot_text_and_original_text() {
        let root = unique_test_root();
        let command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Correction Planning",
            "helo launch plan",
        );

        let snapshot = correct_transcript_segment_for_app_root(
            &root,
            &command_state.snapshot_state(),
            "meeting-1",
            "meeting-1-segment-1",
            "hello launch plan",
            2_500,
        )
        .expect("correct transcript segment");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["selectedMeetingId"], "meeting-1");
        assert_eq!(json["meetings"][0]["transcriptText"], "hello launch plan");
        assert_eq!(
            json["meetings"][0]["segments"][0]["text"],
            "hello launch plan"
        );
        assert_eq!(
            json["meetings"][0]["segments"][0]["originalText"],
            "helo launch plan"
        );

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
        assert_eq!(json["exportCommand"]["format"], "json");
        assert_eq!(json["meetings"][0]["exportState"]["path"], exported_path);
        assert_eq!(json["meetings"][0]["exportState"]["format"], "json");
        assert_eq!(export.meeting_id, "meeting-1");
        assert_eq!(export.segments[0].text, "export this transcript");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn export_meeting_command_writes_markdown_and_srt_and_exposes_format_state() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Export Planning",
            "export this transcript",
        );

        let markdown_snapshot = export_meeting_for_app_root(
            &root,
            &mut command_state,
            "meeting-1",
            ExportFormat::Markdown,
        )
        .expect("export markdown");
        let markdown_json =
            serde_json::to_value(&markdown_snapshot).expect("serialize markdown snapshot");
        let markdown_path = markdown_json["exportCommand"]["path"]
            .as_str()
            .expect("markdown export path");

        assert_eq!(markdown_json["exportCommand"]["state"], "exported");
        assert_eq!(markdown_json["exportCommand"]["format"], "markdown");
        assert_eq!(
            markdown_json["meetings"][0]["exportState"]["path"],
            markdown_path
        );
        assert_eq!(
            markdown_json["meetings"][0]["exportState"]["format"],
            "markdown"
        );
        assert_eq!(
            fs::read_to_string(markdown_path).expect("read markdown"),
            "- [00:00] export this transcript"
        );

        let srt_snapshot =
            export_meeting_for_app_root(&root, &mut command_state, "meeting-1", ExportFormat::Srt)
                .expect("export srt");
        let srt_json = serde_json::to_value(&srt_snapshot).expect("serialize srt snapshot");
        let srt_path = srt_json["exportCommand"]["path"]
            .as_str()
            .expect("srt export path");

        assert_eq!(srt_json["exportCommand"]["state"], "exported");
        assert_eq!(srt_json["exportCommand"]["format"], "srt");
        assert_eq!(srt_json["meetings"][0]["exportState"]["path"], srt_path);
        assert_eq!(srt_json["meetings"][0]["exportState"]["format"], "srt");
        assert_eq!(
            fs::read_to_string(srt_path).expect("read srt"),
            "1\n00:00:00,000 --> 00:00:01,200\nexport this transcript\n"
        );

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
    fn desktop_snapshot_finalizes_pending_delete_intent_and_reports_cleanup() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Pending Delete",
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
        let manifest_path = root.join("meetings/meeting-1/manifest.json");
        let store = open_store(&root).expect("open store");
        store
            .write_recoverable_artifact_manifest(
                "meeting-1",
                "meeting-1-session-1",
                "meeting-1-artifact-1",
                "meetings/meeting-1/audio/imported.wav",
                "sha256:meeting-1",
            )
            .expect("write recoverable manifest");
        store
            .tombstone_audio_artifact("meeting-1-artifact-1")
            .expect("tombstone artifact");
        store
            .update_meeting_status("meeting-1", MeetingStatus::Deleted, None)
            .expect("mark meeting deleted");

        let snapshot =
            desktop_snapshot_for_app_root_with_state(&root, &command_state.snapshot_state())
                .expect("snapshot finalizes pending delete");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert!(!private_path.exists());
        assert!(!manifest_path.exists());
        assert!(PathBuf::from(&exported_path).exists());
        assert_eq!(json["deleteCommand"]["state"], "deleted");
        assert_eq!(json["deleteCommand"]["meetingId"], "meeting-1");
        assert_eq!(json["deleteCommand"]["remainingExports"][0], exported_path);
        let deleted_artifact = json["deleteCommand"]["deletedPrivateArtifacts"][0]
            .as_str()
            .expect("deleted private artifact");
        assert!(deleted_artifact.ends_with("meetings/meeting-1/audio/imported.wav"));
        assert!(json["meetings"]
            .as_array()
            .expect("meetings")
            .iter()
            .all(|meeting| meeting["id"] != "meeting-1"));
        let reopened = open_store(&root).expect("reopen store");
        assert_eq!(reopened.count("audio_artifacts").expect("artifacts"), 0);
        assert_eq!(
            reopened
                .count("recording_sessions")
                .expect("recording sessions"),
            0
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn desktop_snapshot_finalizes_pending_raw_audio_retention_cleanup() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        fs::create_dir_all(&source_root).expect("source dir");
        let source_path = source_root.join("imported.wav");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let import_snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Imported transcript".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = import_snapshot.recording.meeting_id.clone();
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query imported artifact")
            .expect("completed imported artifact");
        let artifact_path = root.join(&artifact.path);
        store
            .tombstone_audio_artifact(&artifact.artifact_id)
            .expect("simulate committed tombstone before file removal");

        desktop_snapshot_for_app_root(&root).expect("snapshot finalizes pending raw cleanup");
        let reopened = open_store(&root).expect("reopen store");

        assert!(!artifact_path.exists());
        assert!(reopened
            .artifact_tombstoned(&artifact.artifact_id)
            .expect("artifact row remains tombstoned"));

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn desktop_snapshot_with_active_recording_does_not_recover_deleted_meeting_summary_job() {
        let root = unique_test_root();
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual(
                "meeting-1",
                "Deleted worker",
                1_700_000_000_000,
            ))
            .expect("insert meeting");
        store
            .update_meeting_status("meeting-1", MeetingStatus::Deleted, None)
            .expect("mark meeting deleted");
        let mut job = ProcessingJob::new(
            "job-deleted-meeting",
            "meeting-1",
            JobKind::Summarize,
            JobStatus::Running,
        );
        job.started_at_ms = Some(1_700_000_001_000);
        store.insert_processing_job(&job).expect("insert job");
        let snapshot_state = DesktopCommandSnapshotState {
            active_recording: Some(ActiveDesktopRecordingSnapshot {
                meeting_id: "active-meeting".to_string(),
                recording_id: "active-recording".to_string(),
                captures_system_audio: false,
                raw_audio_retention_policy: RawAudioRetentionPolicy::Retain,
            }),
            ..Default::default()
        };

        let snapshot =
            desktop_snapshot_for_app_root_with_state(&root, &snapshot_state).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let reopened = open_store(&root).expect("reopen store");

        assert!(json["summaryJob"].is_null());
        assert_eq!(
            reopened
                .processing_job("job-deleted-meeting")
                .expect("deleted meeting job")
                .status,
            JobStatus::Running
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn delete_meeting_rejects_active_recording_meeting_without_corrupting_state() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;
        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Do not delete active".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();

        let snapshot = delete_meeting_for_app_root(&root, &mut command_state, &meeting_id)
            .expect("active delete returns visible failure snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert!(command_state.active_recording.is_some());
        assert_eq!(json["recording"]["meeting_id"], meeting_id);
        assert_eq!(json["recording"]["state"], "Recording");
        assert_eq!(json["deleteCommand"]["state"], "failed");
        assert_eq!(json["deleteCommand"]["meetingId"], meeting_id);
        assert!(json["deleteCommand"]["message"]
            .as_str()
            .expect("delete message")
            .contains("active recording"));
        assert!(json["meetings"]
            .as_array()
            .expect("meetings")
            .iter()
            .any(|meeting| meeting["id"] == meeting_id));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_export_and_delete_commands_do_not_hold_command_mutex_during_store_work() {
        let source = include_str!("main.rs");
        let old_rename_call = concat!(
            "rename_meeting_for_app_root(&app_root, ",
            "&mut command_state"
        );
        let old_export_call = concat!(
            "export_meeting_json_for_app_root(&app_root, ",
            "&mut command_state"
        );
        let old_delete_call = concat!(
            "delete_meeting_for_app_root(&app_root, ",
            "&mut command_state"
        );

        assert!(!source.contains(old_rename_call));
        assert!(!source.contains(old_export_call));
        assert!(!source.contains(old_delete_call));
    }

    #[test]
    fn builder_registers_window_close_recording_shutdown_handler() {
        let source = include_str!("main.rs");

        assert!(source.contains(".on_window_event(cancel_active_recording_on_window_close)"));
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
    fn active_recording_snapshot_does_not_run_startup_repair_on_live_manifest() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = StartedFileMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;

        let snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Still recording".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording with live file");
        let meeting_id = snapshot.recording.meeting_id.clone();
        let recording_id = snapshot.recording.recording_id.expect("recording id");
        let store = open_store(&root).expect("open store");

        assert!(command_state.active_recording.is_some());
        assert_eq!(
            store.meeting_status(&meeting_id).expect("meeting status"),
            "Recording"
        );
        assert_eq!(
            store
                .recording_session_status(&recording_id)
                .expect("session status"),
            "Recording"
        );
        assert_eq!(
            store
                .artifact_recovery_status(&artifact_id(&recording_id))
                .expect("artifact status"),
            curiosity_store::RepairStatus::NotNeeded
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn cancel_active_recording_consumes_recorder_and_prevents_startup_repair_success() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = StartedFileMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;
        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Cancel instead of recover".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording with live file");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let recording_id = start_snapshot.recording.recording_id.expect("recording id");

        let snapshot = cancel_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            started_at_ms + 250,
            "user canceled active recording",
        )
        .expect("cancel recording");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let restarted_store = open_store_with_startup_repair(&root).expect("open repaired store");

        assert!(command_state.active_recording.is_none());
        assert_eq!(json["recording"]["state"], "Interrupted");
        assert_eq!(
            restarted_store
                .recording_session_status(&recording_id)
                .expect("session status"),
            "Failed"
        );
        assert!(restarted_store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query completed artifact")
            .is_none());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn window_shutdown_cancels_active_recording_before_startup_repair() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let factory = StartedFileMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;
        let start_snapshot = {
            let mut state = command_state.lock().expect("command state");
            start_microphone_recording_for_app_root(
                &root,
                &mut state,
                Some("Close window while recording".to_string()),
                started_at_ms,
                &factory,
            )
            .expect("start recording with live file")
        };
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        let recording_id = start_snapshot.recording.recording_id.expect("recording id");

        let canceled =
            cancel_active_recording_for_shutdown(&root, &command_state, started_at_ms + 250)
                .expect("shutdown cancel should produce state");
        let restarted_store = open_store_with_startup_repair(&root).expect("open repaired store");

        assert_eq!(canceled.state, CommandRecordingState::Interrupted);
        assert!(command_state
            .lock()
            .expect("command state")
            .active_recording
            .is_none());
        assert_eq!(
            restarted_store
                .recording_session_status(&recording_id)
                .expect("session status"),
            "Failed"
        );
        assert!(restarted_store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query completed artifact")
            .is_none());

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
    fn start_microphone_recording_captures_current_raw_audio_retention_setting() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMicrophoneRecorderFactory;
        let started_at_ms = 1_700_000_000_000;
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save retention");

        let snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Recorded locally".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start recording");
        let recording_id = snapshot
            .recording
            .recording_id
            .clone()
            .expect("recording id");
        let store = open_store(&root).expect("open store");

        assert_eq!(
            snapshot.recording.raw_audio_retention,
            RawAudioRetentionPolicy::DeleteAfterTranscription
        );
        assert_eq!(
            store
                .recording_session_raw_audio_retention_policy(&recording_id)
                .expect("session policy"),
            DomainRawAudioRetentionPolicy::DeleteAfterTranscription
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn import_audio_file_persists_private_completed_imported_wav_artifact() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        let source_path = source_root.join("customer-call.wav");
        fs::create_dir_all(&source_root).expect("source dir");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();

        let snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Customer call".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = snapshot.recording.meeting_id.clone();
        let recording_id = snapshot
            .recording
            .recording_id
            .clone()
            .expect("recording id");
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query imported artifact")
            .expect("completed imported artifact");
        let copied_path = root.join(&artifact.path);
        let source_sha256 = sha256_for_readable_file(&source_path).expect("source hash");

        assert_eq!(snapshot.recording.state, CommandRecordingState::Complete);
        assert_eq!(
            snapshot.recording.storage_location.app_private_path,
            format!("meetings/{meeting_id}/audio")
        );
        assert_eq!(
            store.meeting_status(&meeting_id).expect("meeting status"),
            "Complete"
        );
        assert_eq!(
            store
                .recording_session_status(&recording_id)
                .expect("session status"),
            "Complete"
        );
        assert_eq!(artifact.kind, "Imported");
        assert_eq!(
            artifact.path,
            format!("meetings/{meeting_id}/audio/{recording_id}/imported.wav")
        );
        assert_ne!(artifact.path, source_path.display().to_string());
        assert!(copied_path.is_file());
        assert!(!root
            .join(imported_temp_artifact_relative_path(
                &meeting_id,
                &recording_id
            ))
            .exists());
        assert_eq!(
            fs::read(&copied_path).expect("copied wav"),
            fs::read(&source_path).expect("source wav")
        );
        assert_eq!(artifact.sha256, source_sha256);
        assert!(!artifact.sha256.starts_with("sha256:pending"));

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn import_audio_file_captures_retention_setting_without_retroactive_changes() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        let source_path = source_root.join("customer-call.wav");
        fs::create_dir_all(&source_root).expect("source dir");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");

        let snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Customer call".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = snapshot.recording.meeting_id.clone();
        let recording_id = snapshot
            .recording
            .recording_id
            .clone()
            .expect("recording id");
        save_raw_audio_retention_policy_for_app_root(&root, "Retain".to_string())
            .expect("save later retain setting");
        let reopened_snapshot =
            desktop_snapshot_for_app_root_with_state(&root, &command_state.snapshot_state())
                .expect("snapshot after settings change");
        let json = serde_json::to_value(&reopened_snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");

        assert_eq!(
            snapshot.recording.raw_audio_retention,
            RawAudioRetentionPolicy::DeleteAfterTranscription
        );
        assert_eq!(
            store
                .recording_session_raw_audio_retention_policy(&recording_id)
                .expect("session policy"),
            DomainRawAudioRetentionPolicy::DeleteAfterTranscription
        );
        assert_eq!(json["meetings"][0]["id"], meeting_id);
        assert_eq!(
            json["meetings"][0]["privacy"]["rawAudioRetention"],
            "DeleteAfterTranscription"
        );
        assert_eq!(json["settings"]["rawAudioRetentionPolicy"], "Retain");

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn import_audio_file_rejects_invalid_sources_without_persisting_rows_or_private_files() {
        let invalid_root = unique_test_root();
        let missing_path = invalid_root.join("missing.wav");
        let directory_path = invalid_root.join("directory.wav");
        let non_wav_path = invalid_root.join("not-a-wav.txt");
        let malformed_wav_path = invalid_root.join("malformed.wav");
        fs::create_dir_all(&directory_path).expect("directory source");
        fs::write(&non_wav_path, b"not wav").expect("non wav source");
        fs::write(&malformed_wav_path, b"RIFFbut not enough").expect("malformed wav source");

        for (source_path, expected_message) in [
            ("".to_string(), "WAV source path is required"),
            (
                missing_path.display().to_string(),
                "WAV source file does not exist",
            ),
            (
                directory_path.display().to_string(),
                "WAV source path must be a file",
            ),
            (
                non_wav_path.display().to_string(),
                "WAV source file must have a .wav extension",
            ),
            (
                malformed_wav_path.display().to_string(),
                "WAV source file has an unsupported WAV header",
            ),
        ] {
            let root = unique_test_root();
            let mut command_state = DesktopCommandState::default();

            let error = import_audio_file_for_app_root(
                &root,
                &mut command_state,
                source_path,
                Some("Rejected".to_string()),
                1_700_000_000_000,
            )
            .expect_err("invalid import should fail");
            let store = open_store(&root).expect("open store");

            assert!(
                error.contains(expected_message),
                "{error:?} did not contain {expected_message:?}"
            );
            assert_eq!(store.count("meetings").expect("meetings"), 0);
            assert_eq!(store.count("recording_sessions").expect("sessions"), 0);
            assert_eq!(store.count("audio_artifacts").expect("artifacts"), 0);
            assert!(!root.join("meetings").exists());
            assert!(command_state.last_recording.is_none());

            fs::remove_dir_all(root).expect("cleanup");
        }

        fs::remove_dir_all(invalid_root).expect("invalid source cleanup");
    }

    #[test]
    fn import_audio_file_rejects_truncated_data_chunk_without_persisting_rows_or_private_files() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        let source_path = source_root.join("truncated-data.wav");
        fs::create_dir_all(&source_root).expect("source dir");
        write_truncated_data_chunk_wav(&source_path);
        let mut command_state = DesktopCommandState::default();

        let error = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Truncated".to_string()),
            1_700_000_000_000,
        )
        .expect_err("truncated data chunk should fail before persistence");
        let store = open_store(&root).expect("open store");

        assert!(
            error.contains("WAV source file has an unsupported WAV header"),
            "{error:?}"
        );
        assert_eq!(store.count("meetings").expect("meetings"), 0);
        assert_eq!(store.count("recording_sessions").expect("sessions"), 0);
        assert_eq!(store.count("audio_artifacts").expect("artifacts"), 0);
        assert!(!root.join("meetings").exists());
        assert!(command_state.last_recording.is_none());

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn import_audio_file_rejects_missing_odd_chunk_pad_without_persisting_rows_or_private_files() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        let source_path = source_root.join("missing-data-pad.wav");
        fs::create_dir_all(&source_root).expect("source dir");
        write_missing_odd_data_chunk_pad_wav(&source_path);
        let mut command_state = DesktopCommandState::default();

        let error = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Missing pad".to_string()),
            1_700_000_000_000,
        )
        .expect_err("missing odd chunk pad should fail before persistence");
        let store = open_store(&root).expect("open store");

        assert!(
            error.contains("WAV source file has an unsupported WAV header"),
            "{error:?}"
        );
        assert_eq!(store.count("meetings").expect("meetings"), 0);
        assert_eq!(store.count("recording_sessions").expect("sessions"), 0);
        assert_eq!(store.count("audio_artifacts").expect("artifacts"), 0);
        assert!(!root.join("meetings").exists());
        assert!(command_state.last_recording.is_none());

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn import_audio_guard_rejects_concurrent_import_and_recording_start() {
        let mut command_state = DesktopCommandState::default();

        command_state.begin_import_audio().expect("begin import");
        let second_import = command_state
            .begin_import_audio()
            .expect_err("second import must be rejected");
        let start_while_importing = command_state
            .begin_recording_start()
            .expect_err("recording start must be rejected while importing");
        command_state.finish_import_audio();
        command_state
            .begin_recording_start()
            .expect("recording start after import finishes");
        let import_while_starting = command_state
            .begin_import_audio()
            .expect_err("import must be rejected during recording startup");

        assert_eq!(
            second_import,
            "Finish the active audio import before importing another WAV."
        );
        assert_eq!(
            start_while_importing,
            "Finish the active audio import before starting a recording."
        );
        assert_eq!(
            import_while_starting,
            "Finish recording startup before importing audio."
        );
    }

    #[test]
    fn import_audio_file_preserves_preexisting_final_artifact_on_destination_collision() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        let source_path = source_root.join("collision-source.wav");
        fs::create_dir_all(&source_root).expect("source dir");
        write_minimal_wav(&source_path);
        let imported_at_ms = 1_700_000_000_000;
        let meeting_id = format!("meeting-{imported_at_ms}");
        let recording_id = format!("recording-{imported_at_ms}");
        let final_path = root.join(imported_artifact_relative_path(&meeting_id, &recording_id));
        let temp_path = root.join(imported_temp_artifact_relative_path(
            &meeting_id,
            &recording_id,
        ));
        fs::create_dir_all(final_path.parent().expect("final parent")).expect("final dir");
        let original_final_bytes = b"preexisting imported wav";
        fs::write(&final_path, original_final_bytes).expect("preexisting final artifact");
        let mut command_state = DesktopCommandState::default();

        let error = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Collision".to_string()),
            imported_at_ms,
        )
        .expect_err("destination collision should reject import");
        let store = open_store(&root).expect("open store");

        assert!(
            error.contains("Imported WAV destination already exists"),
            "{error:?}"
        );
        assert_eq!(
            fs::read(&final_path).expect("final artifact after failed import"),
            original_final_bytes
        );
        assert!(!temp_path.exists());
        assert_eq!(store.count("meetings").expect("meetings"), 0);
        assert_eq!(store.count("recording_sessions").expect("sessions"), 0);
        assert_eq!(store.count("audio_artifacts").expect("artifacts"), 0);
        assert!(command_state.last_recording.is_none());

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn delete_meeting_removes_imported_private_copy_but_preserves_original_source_file() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        let source_path = source_root.join("original.wav");
        fs::create_dir_all(&source_root).expect("source dir");
        write_minimal_wav(&source_path);
        let original_bytes = fs::read(&source_path).expect("original bytes");
        let mut command_state = DesktopCommandState::default();
        let snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Delete imported".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = snapshot.recording.meeting_id.clone();
        let recording_id = snapshot
            .recording
            .recording_id
            .clone()
            .expect("recording id");
        let copied_path = root.join(format!(
            "meetings/{meeting_id}/audio/{recording_id}/imported.wav"
        ));

        let delete_snapshot = delete_meeting_for_app_root(&root, &mut command_state, &meeting_id)
            .expect("delete imported meeting");
        let json = serde_json::to_value(&delete_snapshot).expect("serialize snapshot");

        assert!(!copied_path.exists());
        assert_eq!(
            fs::read(&source_path).expect("source after delete"),
            original_bytes
        );
        assert_eq!(json["deleteCommand"]["state"], "deleted");
        let deleted_artifact = json["deleteCommand"]["deletedPrivateArtifacts"][0]
            .as_str()
            .expect("deleted private artifact");
        assert!(deleted_artifact.ends_with(&format!(
            "meetings/{meeting_id}/audio/{recording_id}/imported.wav"
        )));

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn completed_audio_manifest_mapping_is_the_store_artifact_boundary() {
        let root = unique_test_root();
        let manifest = audio_manifest_for_test(
            &root,
            "recording-1",
            StreamKind::Microphone,
            "meetings/meeting-1/audio/recording-1/raw-mic.wav",
            "sha256:mic",
        );

        let mapped = completed_audio_artifacts_from_manifest(
            &root,
            "meeting-1",
            "recording-1",
            &[StreamKind::Microphone],
            &manifest,
        )
        .expect("map completed artifacts");

        assert_eq!(mapped.completed_streams, vec![StreamKind::Microphone]);
        assert_eq!(
            mapped.completed_artifacts,
            vec![CompletedAudioArtifact {
                artifact_id: "artifact-recording-1".to_string(),
                sha256: "sha256:mic".to_string(),
            }]
        );

        let outside_manifest = audio_manifest_for_test(
            &root,
            "recording-1",
            StreamKind::Microphone,
            "../outside/raw-mic.wav",
            "sha256:outside",
        );
        let outside_error = completed_audio_artifacts_from_manifest(
            &root,
            "meeting-1",
            "recording-1",
            &[StreamKind::Microphone],
            &outside_manifest,
        )
        .expect_err("outside paths must fail before store mutation");
        assert!(outside_error.contains("outside private app storage"));

        let unowned_manifest = audio_manifest_for_test(
            &root,
            "recording-1",
            StreamKind::SystemAudio,
            "meetings/meeting-1/audio/recording-1/raw-system.wav",
            "sha256:system",
        );
        let unowned_error = completed_audio_artifacts_from_manifest(
            &root,
            "meeting-1",
            "recording-1",
            &[StreamKind::Microphone],
            &unowned_manifest,
        )
        .expect_err("unowned streams must fail before store mutation");
        assert!(unowned_error.contains("was not part of the active recording"));

        let _ = fs::remove_dir_all(root);
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

        let restarted_store = open_store_with_startup_repair(&root).expect("open repaired store");
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

        let restarted_store = open_store_with_startup_repair(&root).expect("open repaired store");
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
    fn transcribe_saved_model_without_path_test_returns_visible_failure_without_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual("meeting-1", "Untested Whisper", 1_000))
            .expect("insert meeting");
        drop(store);
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"not a real model").expect("model file");
        save_whisper_model_path_for_app_root(&root, model_path.to_string_lossy().to_string())
            .expect("save whisper");
        let settings = app_settings_for_app_root(&root).expect("settings");

        let snapshot = transcription_readiness_failure_snapshot_for_app_root(
            &root,
            &command_state,
            "meeting-1",
            &settings,
        )
        .expect("readiness snapshot")
        .expect("untested model should fail readiness");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store after failure");

        assert_eq!(json["model"]["kind"], "untested");
        assert_eq!(json["transcription"]["state"], "Failed");
        assert_eq!(
            json["transcription"]["failure"]["code"],
            "model_path_untested"
        );
        assert!(json["transcription"]["failure"]["setupGuidance"]
            .as_str()
            .expect("setup guidance")
            .contains("Run Test path"));
        assert_eq!(json["transcriptionJob"], serde_json::Value::Null);
        assert!(store
            .active_transcription_job_for_meeting("meeting-1")
            .expect("active job")
            .is_none());
        assert!(store
            .transcript_segments("meeting-1")
            .expect("transcript segments")
            .is_empty());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unsupported_saved_whisper_paths_are_typed_blocked_and_ignore_legacy_valid_evidence() {
        for file_name in ["notes.txt", "extensionless"] {
            let root = unique_test_root();
            fs::create_dir_all(&root).expect("test root");
            let model_path = root.join(file_name);
            let model_bytes = b"not a whisper model";
            fs::write(&model_path, model_bytes).expect("unsupported model path");
            let model_path_string = model_path.to_string_lossy().to_string();
            save_whisper_model_path_for_app_root(&root, model_path_string.clone())
                .expect("save unsupported whisper path");
            let modified_at_ms =
                file_modified_at_ms(&fs::metadata(&model_path).expect("model metadata"))
                    .expect("model modified time");
            let store = open_store(&root).expect("open store");
            store
                .save_whisper_path_test_evidence(&WhisperPathTestEvidence {
                    tested_path: model_path_string.clone(),
                    tested_at_ms: 1_700_000_001_000,
                    state: "Valid".to_string(),
                    file_size_bytes: Some(model_bytes.len() as u64),
                    sha256: Some(format!("{:x}", Sha256::digest(model_bytes))),
                    failure_detail: None,
                })
                .expect("legacy path-test evidence");
            store
                .save_whisper_transcription_compatibility_evidence(
                    &WhisperTranscriptionCompatibilityEvidence {
                        model_path: model_path_string.clone(),
                        used_at_ms: 1_700_000_002_000,
                        provider: "local-whisper".to_string(),
                        model_name: file_name.to_string(),
                        meeting_id: "meeting-1".to_string(),
                        model_run_id: "run-1".to_string(),
                        transcript_version_id: "version-1".to_string(),
                        segment_count: 1,
                        file_size_bytes: model_bytes.len() as u64,
                        modified_at_ms,
                    },
                )
                .expect("legacy transcription compatibility evidence");
            drop(store);

            let snapshot = desktop_snapshot_for_app_root(&root).expect("desktop snapshot");
            let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

            assert_eq!(json["model"]["kind"], "unsupported");
            assert_eq!(json["setupGuidance"]["whisper"]["state"], "UnsupportedFile");
            assert_eq!(
                json["setupGuidance"]["whisper"]["lastPathTest"],
                serde_json::Value::Null
            );
            assert_eq!(
                json["setupGuidance"]["whisper"]["lastSuccessfulTranscription"],
                serde_json::Value::Null
            );
            assert!(json["setupGuidance"]["whisper"]["setupGuidance"]
                .as_str()
                .expect("setup guidance")
                .contains(".bin or .gguf"));
            assert!(!json["setupGuidance"]["whisper"]["setupGuidance"]
                .as_str()
                .expect("setup guidance")
                .contains("Test path"));

            fs::remove_dir_all(root).expect("cleanup");
        }
    }

    #[test]
    fn transcribe_unsupported_saved_whisper_path_returns_typed_failure_without_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let store = open_store(&root).expect("open store");
        store
            .insert_meeting(&Meeting::new_manual(
                "meeting-1",
                "Unsupported Whisper",
                1_000,
            ))
            .expect("insert meeting");
        drop(store);
        let model_path = root.join("notes.txt");
        fs::write(&model_path, b"not a whisper model").expect("unsupported model file");
        save_whisper_model_path_for_app_root(&root, model_path.to_string_lossy().to_string())
            .expect("save unsupported whisper");
        let settings = app_settings_for_app_root(&root).expect("settings");

        let snapshot = transcription_readiness_failure_snapshot_for_app_root(
            &root,
            &command_state,
            "meeting-1",
            &settings,
        )
        .expect("readiness snapshot")
        .expect("unsupported model should fail readiness");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store after failure");

        assert_eq!(json["model"]["kind"], "unsupported");
        assert_eq!(json["transcription"]["state"], "Failed");
        assert_eq!(
            json["transcription"]["failure"]["code"],
            "unsupported_model_file"
        );
        assert!(json["transcription"]["failure"]["setupGuidance"]
            .as_str()
            .expect("setup guidance")
            .contains(".bin or .gguf"));
        assert!(!json["transcription"]["failure"]["setupGuidance"]
            .as_str()
            .expect("setup guidance")
            .contains("Test path"));
        assert_eq!(json["transcriptionJob"], serde_json::Value::Null);
        assert!(store
            .active_transcription_job_for_meeting("meeting-1")
            .expect("active job")
            .is_none());
        assert!(store
            .transcript_segments("meeting-1")
            .expect("transcript segments")
            .is_empty());

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
    fn successful_transcription_records_historical_whisper_compatibility_without_readiness() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let meeting_id = seed_stopped_fake_recording(&root, &mut command_state);
        let model_path = root.join("fixture-whisper.bin");
        let model_bytes = b"fixture model";
        fs::write(&model_path, model_bytes).expect("model file");
        save_whisper_model_path_for_app_root(&root, model_path.to_string_lossy().to_string())
            .expect("save whisper path");

        let snapshot = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path.clone(),
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
                0,
                1_200,
                "compatibility transcript",
            )]),
            1_700_000_001_000,
        )
        .expect("transcribe meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let evidence = &json["setupGuidance"]["whisper"]["lastSuccessfulTranscription"];
        let model_modified_at_ms =
            file_modified_at_ms(&fs::metadata(&model_path).expect("model metadata"))
                .expect("model modified time");

        assert_eq!(json["transcription"]["state"], "Complete");
        assert_eq!(json["model"]["kind"], "untested");
        assert_eq!(
            json["setupGuidance"]["whisper"]["lastPathTest"],
            serde_json::Value::Null
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["message"],
            "Whisper model path is readable and has completed transcription before."
        );
        assert!(json["setupGuidance"]["whisper"]["compatibilityNote"]
            .as_str()
            .expect("compatibility note")
            .contains("historical evidence"));
        assert_eq!(evidence["modelPath"], model_path.to_string_lossy().as_ref());
        assert_eq!(evidence["usedAtMs"], 1_700_000_001_000_u64);
        assert_eq!(evidence["provider"], "local-whisper");
        assert_eq!(evidence["modelName"], "fixture-whisper.bin");
        assert_eq!(evidence["meetingId"], meeting_id);
        assert_eq!(evidence["segmentCount"], 1_u64);
        assert_eq!(evidence["fileSizeBytes"], model_bytes.len() as u64);
        assert_eq!(evidence["modifiedAtMs"], model_modified_at_ms);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn desktop_snapshot_hides_stale_successful_transcription_evidence_for_changed_model_metadata() {
        let root = unique_test_root();
        fs::create_dir_all(&root).expect("test root");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");
        save_whisper_model_path_for_app_root(&root, model_path.to_string_lossy().to_string())
            .expect("save whisper path");
        let store = open_store(&root).expect("open store");
        store
            .save_whisper_transcription_compatibility_evidence(
                &WhisperTranscriptionCompatibilityEvidence {
                    model_path: model_path.to_string_lossy().to_string(),
                    used_at_ms: 1_700_000_001_000,
                    provider: "local-whisper".to_string(),
                    model_name: "fixture-whisper.bin".to_string(),
                    meeting_id: "meeting-1".to_string(),
                    model_run_id: "run-1".to_string(),
                    transcript_version_id: "version-1".to_string(),
                    segment_count: 1,
                    file_size_bytes: b"fixture model".len() as u64,
                    modified_at_ms: 0,
                },
            )
            .expect("save stale evidence");

        let snapshot = desktop_snapshot_for_app_root(&root).expect("snapshot");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(
            json["setupGuidance"]["whisper"]["lastSuccessfulTranscription"],
            serde_json::Value::Null
        );
        assert_eq!(
            json["setupGuidance"]["whisper"]["message"],
            "Whisper model path is readable; compatibility is not verified."
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcribe_delete_after_recording_removes_raw_audio_after_persisting_transcript() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        fs::create_dir_all(&source_root).expect("source dir");
        let source_path = source_root.join("imported.wav");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let import_snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Imported transcript".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = import_snapshot.recording.meeting_id.clone();
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query imported artifact")
            .expect("completed imported artifact");
        let artifact_path = root.join(&artifact.path);
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");

        let snapshot = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
                0,
                1_200,
                "delete after transcript",
            )]),
            1_700_000_001_000,
        )
        .expect("transcribe imported meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let reopened = open_store(&root).expect("reopen store");

        assert_eq!(json["transcription"]["state"], "Complete");
        assert_eq!(
            json["meetings"][0]["transcriptText"],
            "delete after transcript"
        );
        assert_eq!(
            json["meetings"][0]["privacy"]["rawAudioRetention"],
            "DeleteAfterTranscription"
        );
        assert!(!artifact_path.exists());
        assert!(reopened
            .artifact_tombstoned(&artifact.artifact_id)
            .expect("artifact tombstoned"));
        assert!(reopened
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query retained artifacts")
            .is_none());
        assert_eq!(
            reopened
                .transcript_segments(&meeting_id)
                .expect("transcript remains")
                .len(),
            1
        );

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn transcribe_delete_after_mixed_recording_removes_all_raw_audio_after_persisting_transcript() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMixedRecorderFactory;
        let started_at_ms = 1_700_000_000_000;
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let start_snapshot = start_microphone_recording_for_app_root(
            &root,
            &mut command_state,
            Some("Delete after full call".to_string()),
            started_at_ms,
            &factory,
        )
        .expect("start mixed recording");
        let meeting_id = start_snapshot.recording.meeting_id.clone();
        stop_microphone_recording_for_app_root(&root, &mut command_state, started_at_ms + 500)
            .expect("stop mixed recording");
        let artifacts = {
            let store = open_store(&root).expect("open store");
            store
                .completed_wav_artifacts_for_transcription(&meeting_id)
                .expect("query mixed artifacts")
        };
        let artifact_paths = artifacts
            .iter()
            .map(|artifact| {
                (
                    artifact.artifact_id.clone(),
                    artifact.kind.clone(),
                    root.join(&artifact.path),
                )
            })
            .collect::<Vec<_>>();
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
        .expect("transcribe mixed delete-after meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let reopened = open_store(&root).expect("reopen store");
        let transcript_segments = reopened
            .transcript_segments(&meeting_id)
            .expect("transcript remains");

        assert_eq!(json["transcription"]["state"], "Complete");
        assert_eq!(
            json["meetings"][0]["privacy"]["rawAudioRetention"],
            "DeleteAfterTranscription"
        );
        assert_eq!(
            artifact_paths
                .iter()
                .map(|(_, kind, _)| kind.as_str())
                .collect::<Vec<_>>(),
            vec!["RawMic", "RawSystem"]
        );
        for (artifact_id, _kind, path) in &artifact_paths {
            assert!(
                !path.exists(),
                "delete-after cleanup should remove selected raw artifact file: {}",
                path.display()
            );
            assert!(reopened
                .artifact_tombstoned(artifact_id)
                .expect("artifact tombstoned"));
        }
        assert!(reopened
            .completed_wav_artifacts_for_transcription(&meeting_id)
            .expect("query retained artifacts")
            .is_empty());
        assert_eq!(transcript_segments.len(), 2);
        assert_eq!(
            transcript_segments
                .iter()
                .map(|segment| segment.source_channel)
                .collect::<Vec<_>>(),
            vec![SourceChannel::Microphone, SourceChannel::System]
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn delete_after_cleanup_failure_after_transcript_persistence_returns_error() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        fs::create_dir_all(&source_root).expect("source dir");
        let source_path = source_root.join("imported.wav");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let import_snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Imported transcript".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = import_snapshot.recording.meeting_id.clone();
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query imported artifact")
            .expect("completed imported artifact");
        let artifact_path = root.join(&artifact.path);
        let artifact_parent = artifact_path.parent().expect("artifact parent");
        fs::set_permissions(artifact_parent, fs::Permissions::from_mode(0o555))
            .expect("make artifact parent read-only");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");

        let error = transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
                0,
                1_200,
                "persisted before cleanup failure",
            )]),
            1_700_000_001_000,
        )
        .expect_err("cleanup failure should not be marked as command success");
        fs::set_permissions(artifact_parent, fs::Permissions::from_mode(0o755))
            .expect("restore artifact parent permissions");
        let reopened = open_store(&root).expect("reopen store");

        assert!(
            error.contains("Raw audio retention cleanup failed"),
            "cleanup failure should be surfaced clearly: {error}"
        );
        assert!(artifact_path.exists());
        assert!(!reopened
            .artifact_tombstoned(&artifact.artifact_id)
            .expect("failed cleanup leaves artifact row retained"));
        assert_eq!(
            reopened
                .transcript_segments(&meeting_id)
                .expect("transcript persisted before cleanup")
                .len(),
            1
        );

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn failed_delete_after_transcription_keeps_raw_audio_retained() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        fs::create_dir_all(&source_root).expect("source dir");
        let source_path = source_root.join("imported.wav");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let import_snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Imported transcript".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = import_snapshot.recording.meeting_id.clone();
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query imported artifact")
            .expect("completed imported artifact");
        let artifact_path = root.join(&artifact.path);

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
        let reopened = open_store(&root).expect("reopen store");

        assert_eq!(json["transcription"]["state"], "Failed");
        assert!(artifact_path.exists());
        assert!(!reopened
            .artifact_tombstoned(&artifact.artifact_id)
            .expect("failed transcription keeps artifact"));

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
    }

    #[test]
    fn transcribe_imported_wav_maps_segments_to_imported_channel() {
        let root = unique_test_root();
        let source_root = unique_test_root();
        fs::create_dir_all(&source_root).expect("source dir");
        let source_path = source_root.join("imported.wav");
        write_minimal_wav(&source_path);
        let mut command_state = DesktopCommandState::default();
        let import_snapshot = import_audio_file_for_app_root(
            &root,
            &mut command_state,
            source_path.display().to_string(),
            Some("Imported transcript".to_string()),
            1_700_000_000_000,
        )
        .expect("import wav");
        let meeting_id = import_snapshot.recording.meeting_id.clone();
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");
        let backend = FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
            0,
            1_200,
            "imported transcript",
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
        .expect("transcribe imported meeting");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["transcription"]["state"], "Complete");
        assert_eq!(json["meetings"][0]["transcriptText"], "imported transcript");
        assert_eq!(
            json["meetings"][0]["segments"][0]["sourceChannel"],
            "Imported"
        );

        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source_root).expect("source cleanup");
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
    fn desktop_snapshot_marks_system_audio_ready_when_persisted_transcript_has_system_segments() {
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
        transcribe_meeting_for_app_root(
            &root,
            &mut command_state,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            PathAwareWhisperBackend,
            1_700_000_001_000,
        )
        .expect("transcribe mixed meeting");

        let restarted_snapshot = desktop_snapshot_for_app_root_with_state(
            &root,
            &DesktopCommandSnapshotState::default(),
        )
        .expect("snapshot after restart");
        let json = serde_json::to_value(&restarted_snapshot).expect("serialize snapshot");

        assert_eq!(json["capture"]["systemAudio"], "Ready");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_ownership_rejects_duplicate_start_and_keeps_running_status_visible() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");

        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        let duplicate = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_001,
        )
        .expect_err("duplicate job should be rejected");
        let duplicate_snapshot = {
            let state = command_state.lock().expect("command state");
            desktop_snapshot_for_app_root_with_state(&root, &state.snapshot_state())
                .expect("duplicate snapshot")
        };
        let duplicate_json =
            serde_json::to_value(&duplicate_snapshot).expect("serialize duplicate");

        assert_eq!(started.kind, CommandJobKind::Transcription);
        assert!(duplicate.contains(&started.id));
        assert_eq!(duplicate_json["transcriptionJob"]["id"], started.id);
        assert_eq!(duplicate_json["transcriptionJob"]["state"], "Running");

        {
            let mut state = command_state.lock().expect("command state");
            state
                .transcription_job
                .as_mut()
                .expect("transcription job")
                .state = CommandJobState::CancelRequested;
        }
        let cancel_requested_duplicate = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_002,
        )
        .expect_err("cancel-requested job should still own the command");
        let cancel_requested_snapshot = {
            let state = command_state.lock().expect("command state");
            desktop_snapshot_for_app_root_with_state(&root, &state.snapshot_state())
                .expect("cancel requested snapshot")
        };
        let cancel_requested_json =
            serde_json::to_value(&cancel_requested_snapshot).expect("serialize duplicate");

        assert!(cancel_requested_duplicate.contains(&started.id));
        assert_eq!(
            cancel_requested_json["transcriptionJob"]["state"],
            "CancelRequested"
        );

        finish_transcription_job_for_app_root(
            &root,
            &command_state,
            started,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(0, 1_200, "job owned")]),
            1_700_000_001_500,
        )
        .expect("finish transcription job");
        let repeated = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_002_000,
        )
        .expect("begin repeated transcription job after completion");

        assert_ne!(
            repeated.id,
            duplicate_json["transcriptionJob"]["id"]
                .as_str()
                .expect("job id")
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_lifecycle_persists_durable_processing_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");

        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        let store = open_store(&root).expect("open store");
        let durable_started = store
            .processing_job(&started.id)
            .expect("durable started job");

        assert_eq!(durable_started.kind, curiosity_domain::JobKind::Transcribe);
        assert_eq!(durable_started.status, curiosity_domain::JobStatus::Running);
        assert_eq!(durable_started.attempts, 1);
        assert_eq!(durable_started.started_at_ms, Some(1_700_000_001_000));
        assert_eq!(
            durable_started.idempotency_key.as_deref(),
            Some(transcription_idempotency_key(&meeting_id).as_str())
        );
        assert!(!durable_started.cancel_requested);

        cancel_transcription_job_for_app_root(&root, &command_state, &started.id)
            .expect("request transcription cancel");
        let durable_cancel = store
            .processing_job(&started.id)
            .expect("durable cancel-requested job");
        assert!(durable_cancel.cancel_requested);

        finish_transcription_job_for_app_root(
            &root,
            &command_state,
            started.clone(),
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
                0,
                1_200,
                "durable canceled",
            )]),
            1_700_000_001_500,
        )
        .expect("finish transcription job");
        let durable_finished = store
            .processing_job(&started.id)
            .expect("durable finished job");

        assert_eq!(
            durable_finished.status,
            curiosity_domain::JobStatus::Canceled
        );
        assert_eq!(durable_finished.finished_at_ms, Some(1_700_000_001_500));
        assert!(!durable_finished.cancel_requested);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_restart_ownership_uses_durable_active_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };

        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        let restarted_command_state = Mutex::new(DesktopCommandState::default());
        let duplicate = begin_transcription_job_for_app_root(
            &root,
            &restarted_command_state,
            &meeting_id,
            1_700_000_001_100,
        )
        .expect_err("durable active job should own transcription after restart");

        assert!(duplicate.contains(&started.id));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_restart_duplicate_recovers_orphan_without_phantom_running_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };

        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        let restarted_command_state = Mutex::new(DesktopCommandState::default());
        let duplicate = begin_transcription_job_for_app_root(
            &root,
            &restarted_command_state,
            &meeting_id,
            1_700_000_001_100,
        )
        .expect_err("durable orphan should reject this duplicate attempt");
        let snapshot = {
            let state = restarted_command_state.lock().expect("command state");
            desktop_snapshot_for_app_root_with_state(&root, &state.snapshot_state())
                .expect("snapshot after duplicate")
        };
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");
        let recovered = store
            .processing_job(&started.id)
            .expect("durable recovered job");

        assert!(duplicate.contains(&started.id));
        assert_eq!(json["transcriptionJob"]["state"], "Recovery");
        assert_eq!(
            json["transcriptionJob"]["lastError"],
            "transcription worker was not running after app restart"
        );
        assert_eq!(recovered.status, curiosity_domain::JobStatus::Recovery);
        assert_eq!(recovered.finished_at_ms, Some(1_700_000_001_100));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_restart_snapshot_recovers_missing_worker() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };

        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        let restarted_snapshot = desktop_snapshot_for_app_root_with_state(
            &root,
            &DesktopCommandSnapshotState::default(),
        )
        .expect("restart snapshot");
        let restarted_json = serde_json::to_value(&restarted_snapshot).expect("serialize restart");
        let store = open_store(&root).expect("open store");
        let recovered = store
            .processing_job(&started.id)
            .expect("durable recovered job");

        assert_eq!(restarted_json["transcriptionJob"]["id"], started.id);
        assert_eq!(restarted_json["transcriptionJob"]["state"], "Recovery");
        assert_eq!(
            restarted_json["transcriptionJob"]["lastError"],
            "transcription worker was not running after app restart"
        );
        assert_eq!(recovered.status, curiosity_domain::JobStatus::Recovery);
        assert!(
            recovered.finished_at_ms.unwrap_or_default() >= 1_700_000_001_000,
            "recovery finish time should be the snapshot recovery time, not the job start time"
        );
        assert_eq!(
            recovered.last_error.as_deref(),
            Some("transcription worker was not running after app restart")
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_cancel_request_marks_snapshot_and_blocks_duplicate_until_finish() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");
        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");

        let cancel_snapshot =
            cancel_transcription_job_for_app_root(&root, &command_state, &started.id)
                .expect("request transcription cancel");
        let cancel_json = serde_json::to_value(&cancel_snapshot).expect("serialize cancel");
        let duplicate = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_001,
        )
        .expect_err("cancel-requested job still owns transcription");

        assert!(duplicate.contains(&started.id));
        assert_eq!(cancel_json["transcriptionJob"]["state"], "CancelRequested");
        assert_eq!(cancel_json["transcriptionJob"]["cancelRequested"], true);

        let finish_snapshot = finish_transcription_job_for_app_root(
            &root,
            &command_state,
            started,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(0, 1_200, "job owned")]),
            1_700_000_001_500,
        )
        .expect("finish transcription job");
        let finish_json = serde_json::to_value(&finish_snapshot).expect("serialize finish");
        let store = open_store(&root).expect("open store");

        assert_eq!(finish_json["transcriptionJob"]["state"], "Canceled");
        assert_eq!(finish_json["transcriptionJob"]["cancelRequested"], false);
        assert!(finish_json["transcription"].is_null());
        assert!(
            store
                .transcript_segments(&meeting_id)
                .expect("query transcript segments")
                .is_empty(),
            "a canceled transcription must not persist completed backend output"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn canceled_delete_after_transcription_job_keeps_raw_audio_retained() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query completed artifact")
            .expect("completed artifact");
        let artifact_path = root.join(&artifact.path);
        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        cancel_transcription_job_for_app_root(&root, &command_state, &started.id)
            .expect("request transcription cancel");

        let snapshot = finish_transcription_job_for_app_root(
            &root,
            &command_state,
            started,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
                0,
                1_200,
                "canceled transcript",
            )]),
            1_700_000_001_500,
        )
        .expect("finish canceled transcription job");
        let json = serde_json::to_value(&snapshot).expect("serialize finish");
        let reopened = open_store(&root).expect("reopen store");

        assert_eq!(json["transcriptionJob"]["state"], "Canceled");
        assert!(json["transcription"].is_null());
        assert!(artifact_path.exists());
        assert!(!reopened
            .artifact_tombstoned(&artifact.artifact_id)
            .expect("canceled transcription keeps artifact"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn delete_after_cleanup_failure_marks_durable_transcription_job_failed() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        save_raw_audio_retention_policy_for_app_root(&root, "DeleteAfterTranscription".to_string())
            .expect("save delete-after retention");
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };
        let store = open_store(&root).expect("open store");
        let artifact = store
            .completed_wav_artifact_for_transcription(&meeting_id)
            .expect("query completed artifact")
            .expect("completed artifact");
        let artifact_path = root.join(&artifact.path);
        let artifact_parent = artifact_path.parent().expect("artifact parent");
        fs::set_permissions(artifact_parent, fs::Permissions::from_mode(0o555))
            .expect("make artifact parent read-only");
        let model_path = root.join("fixture-whisper.bin");
        fs::write(&model_path, b"fixture model").expect("model file");
        let started = begin_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("begin transcription job");
        let job_id = started.id.clone();

        let finish_result = finish_transcription_job_for_app_root(
            &root,
            &command_state,
            started,
            &meeting_id,
            model_path,
            "fixture-whisper.bin",
            FakeWhisperBackend::new(vec![WhisperBackendSegment::new(
                0,
                1_200,
                "persisted before durable cleanup failure",
            )]),
            1_700_000_001_500,
        );
        fs::set_permissions(artifact_parent, fs::Permissions::from_mode(0o755))
            .expect("restore artifact parent permissions");
        let error = finish_result.expect_err("cleanup failure should fail the durable job finish");
        let reopened = open_store(&root).expect("reopen store");
        let durable_job = reopened
            .processing_job(&job_id)
            .expect("durable failed transcription job");

        assert!(
            error.contains("Raw audio retention cleanup failed"),
            "cleanup failure should be returned to the worker: {error}"
        );
        assert_eq!(durable_job.status, curiosity_domain::JobStatus::Failed);
        assert_eq!(durable_job.finished_at_ms, Some(1_700_000_001_500));
        assert!(
            durable_job
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("Raw audio retention cleanup failed"),
            "durable job should persist the cleanup error: {:?}",
            durable_job.last_error
        );
        assert!(artifact_path.exists());
        assert!(!reopened
            .artifact_tombstoned(&artifact.artifact_id)
            .expect("failed cleanup leaves artifact row retained"));
        assert_eq!(
            reopened
                .transcript_segments(&meeting_id)
                .expect("transcript persisted before cleanup")
                .len(),
            1
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_start_returns_running_snapshot_before_worker_persists_transcript() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        let meeting_id = {
            let mut state = command_state.lock().expect("command state");
            seed_stopped_fake_recording(&root, &mut state)
        };

        let (started, snapshot) = start_transcription_job_for_app_root(
            &root,
            &command_state,
            &meeting_id,
            1_700_000_001_000,
        )
        .expect("start transcription job");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");

        assert_eq!(json["transcriptionJob"]["id"], started.id);
        assert_eq!(json["transcriptionJob"]["state"], "Running");
        assert!(json["transcription"].is_null());
        assert!(
            store
                .transcript_segments(&meeting_id)
                .expect("query transcript segments")
                .is_empty(),
            "starting the job must return before worker persistence"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transcription_job_start_marks_job_failed_when_running_snapshot_cannot_be_built() {
        let root = unique_test_root();
        fs::write(&root, b"not a directory").expect("create invalid app root file");
        let command_state = Mutex::new(DesktopCommandState::default());

        let error = start_transcription_job_for_app_root(
            &root,
            &command_state,
            "meeting-1",
            1_700_000_001_000,
        )
        .expect_err("snapshot creation should fail for invalid app root");
        let job = command_state
            .lock()
            .expect("command state")
            .transcription_job
            .clone()
            .expect("failed transcription job remains visible");

        assert!(!error.is_empty());
        assert_eq!(job.state, CommandJobState::Failed);
        assert!(!job.cancel_requested);

        fs::remove_file(root).expect("cleanup");
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

    #[test]
    fn summary_job_ownership_rejects_duplicate_start_and_keeps_running_status_visible() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "summarize this transcript",
        );

        let started =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect("begin summary job");
        let duplicate =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_001)
                .expect_err("duplicate summary job should be rejected");
        let duplicate_snapshot = {
            let state = command_state.lock().expect("command state");
            desktop_snapshot_for_app_root_with_state(&root, &state.snapshot_state())
                .expect("duplicate snapshot")
        };
        let duplicate_json =
            serde_json::to_value(&duplicate_snapshot).expect("serialize duplicate");

        assert_eq!(started.kind, CommandJobKind::Summary);
        assert!(duplicate.contains(&started.id));
        assert_eq!(duplicate_json["summaryJob"]["id"], started.id);
        assert_eq!(duplicate_json["summaryJob"]["state"], "Running");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn summary_job_lifecycle_persists_durable_processing_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "summarize this transcript",
        );

        let succeeded =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect("begin summary job");
        let store = open_store(&root).expect("open store");
        let durable_started = store
            .processing_job(&succeeded.id)
            .expect("durable started summary job");
        assert_eq!(durable_started.kind, curiosity_domain::JobKind::Summarize);
        assert_eq!(durable_started.status, curiosity_domain::JobStatus::Running);
        assert_eq!(durable_started.attempts, 1);
        assert_eq!(durable_started.started_at_ms, Some(1_700_000_001_000));
        assert_eq!(
            durable_started.idempotency_key.as_deref(),
            Some(summary_idempotency_key("meeting-1").as_str())
        );
        assert!(!durable_started.cancel_requested);

        let success_transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Durable summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[{\"segment_id\":\"meeting-1-segment-1\",\"start_ms\":0,\"end_ms\":1200}]}"}"#,
        );
        let success_client =
            LocalOllamaTextClient::new("http://127.0.0.1:11434", success_transport);
        finish_summary_job_for_app_root_with_client(
            &root,
            &command_state,
            succeeded.clone(),
            "meeting-1",
            success_client,
            "qwen3.6:27b",
            1_700_000_001_500,
        )
        .expect("finish successful summary job");
        let durable_succeeded = store
            .processing_job(&succeeded.id)
            .expect("durable succeeded summary job");
        assert_eq!(
            durable_succeeded.status,
            curiosity_domain::JobStatus::Succeeded
        );
        assert_eq!(durable_succeeded.finished_at_ms, Some(1_700_000_001_500));
        assert_eq!(durable_succeeded.last_error, None);
        assert!(!durable_succeeded.cancel_requested);

        let failed =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_002_000)
                .expect("begin failing summary job");
        let failure_transport = RecordingOllamaTransport::generate_error("connection refused");
        let failure_client =
            LocalOllamaTextClient::new("http://127.0.0.1:11434", failure_transport);
        finish_summary_job_for_app_root_with_client(
            &root,
            &command_state,
            failed.clone(),
            "meeting-1",
            failure_client,
            "qwen3.6:27b",
            1_700_000_002_500,
        )
        .expect("finish failed summary job as visible command failure");
        let durable_failed = store
            .processing_job(&failed.id)
            .expect("durable failed summary job");
        assert_eq!(durable_failed.status, curiosity_domain::JobStatus::Failed);
        assert_eq!(durable_failed.finished_at_ms, Some(1_700_000_002_500));
        assert!(
            durable_failed
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("connection refused"),
            "failed durable summary jobs should keep the actionable provider error"
        );
        assert!(!durable_failed.cancel_requested);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn summary_job_cancel_request_marks_snapshot_and_blocks_duplicate() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "summarize this transcript",
        );
        let started =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect("begin summary job");

        let cancel_snapshot = cancel_summary_job_for_app_root(&root, &command_state, &started.id)
            .expect("request summary cancel");
        let cancel_json = serde_json::to_value(&cancel_snapshot).expect("serialize cancel");
        let store = open_store(&root).expect("open store");
        let durable_cancel = store
            .processing_job(&started.id)
            .expect("durable cancel-requested summary job");
        let duplicate =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_001)
                .expect_err("cancel-requested summary still owns command");

        assert!(duplicate.contains(&started.id));
        assert_eq!(cancel_json["summaryJob"]["state"], "CancelRequested");
        assert_eq!(cancel_json["summaryJob"]["cancelRequested"], true);
        assert!(durable_cancel.cancel_requested);

        let transport = RecordingOllamaTransport::generate_response(
            r#"{"response":"{\"summary\":\"Canceled summary\",\"decisions\":[],\"action_items\":[],\"questions\":[],\"citations\":[{\"segment_id\":\"meeting-1-segment-1\",\"start_ms\":0,\"end_ms\":1200}]}"}"#,
        );
        let client = LocalOllamaTextClient::new("http://127.0.0.1:11434", transport);
        let finish_snapshot = finish_summary_job_for_app_root_with_client(
            &root,
            &command_state,
            started.clone(),
            "meeting-1",
            client,
            "qwen3.6:27b",
            1_700_000_001_500,
        )
        .expect("finish canceled summary job");
        let finish_json = serde_json::to_value(&finish_snapshot).expect("serialize finish");
        let durable_finished = store
            .processing_job(&started.id)
            .expect("durable canceled summary job");

        assert_eq!(finish_json["summaryJob"]["state"], "Canceled");
        assert_eq!(finish_json["summaryJob"]["cancelRequested"], false);
        assert!(finish_json["analysisCommand"].is_null());
        assert!(finish_json["meetings"][0]["analysis"].is_null());
        assert_eq!(
            durable_finished.status,
            curiosity_domain::JobStatus::Canceled
        );
        assert_eq!(durable_finished.finished_at_ms, Some(1_700_000_001_500));
        assert!(!durable_finished.cancel_requested);
        assert!(
            store
                .current_analysis_result("meeting-1")
                .expect("query analysis result")
                .is_none(),
            "a canceled summary must not persist completed analyzer output"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn summary_job_restart_duplicate_recovers_orphan_without_phantom_running_job() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "summarize this transcript",
        );

        let started =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect("begin summary job");
        let restarted_command_state = Mutex::new(DesktopCommandState::default());
        let duplicate = begin_summary_job_for_app_root(
            &root,
            &restarted_command_state,
            "meeting-1",
            1_700_000_001_100,
        )
        .expect_err("durable orphan should reject this duplicate summary attempt");
        let snapshot = {
            let state = restarted_command_state.lock().expect("command state");
            desktop_snapshot_for_app_root_with_state(&root, &state.snapshot_state())
                .expect("snapshot after duplicate")
        };
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");
        let store = open_store(&root).expect("open store");
        let recovered = store
            .processing_job(&started.id)
            .expect("durable recovered summary job");

        assert!(duplicate.contains(&started.id));
        assert_eq!(json["summaryJob"]["id"], started.id);
        assert_eq!(json["summaryJob"]["state"], "Recovery");
        assert_eq!(
            json["summaryJob"]["lastError"],
            "summary worker was not running after app restart"
        );
        assert_eq!(recovered.status, curiosity_domain::JobStatus::Recovery);
        assert_eq!(recovered.finished_at_ms, Some(1_700_000_001_100));
        assert_eq!(
            recovered.last_error.as_deref(),
            Some("summary worker was not running after app restart")
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn summary_job_restart_snapshot_recovers_missing_worker() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "summarize this transcript",
        );

        let started =
            begin_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect("begin summary job");
        let restarted_snapshot = desktop_snapshot_for_app_root_with_state(
            &root,
            &DesktopCommandSnapshotState::default(),
        )
        .expect("restart snapshot");
        let restarted_json = serde_json::to_value(&restarted_snapshot).expect("serialize restart");
        let store = open_store(&root).expect("open store");
        let recovered = store
            .processing_job(&started.id)
            .expect("durable recovered summary job");

        assert_eq!(restarted_json["summaryJob"]["id"], started.id);
        assert_eq!(restarted_json["summaryJob"]["state"], "Recovery");
        assert_eq!(
            restarted_json["summaryJob"]["lastError"],
            "summary worker was not running after app restart"
        );
        assert_eq!(recovered.status, curiosity_domain::JobStatus::Recovery);
        assert!(
            recovered.finished_at_ms.unwrap_or_default() >= 1_700_000_001_000,
            "recovery finish time should be the snapshot recovery time, not the job start time"
        );
        assert_eq!(
            recovered.last_error.as_deref(),
            Some("summary worker was not running after app restart")
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn summary_job_start_returns_running_snapshot_before_worker_persists_analysis() {
        let root = unique_test_root();
        let command_state = Mutex::new(DesktopCommandState::default());
        seed_transcribed_meeting_with_private_artifact(
            &root,
            "meeting-1",
            "Summary Planning",
            "summarize this transcript",
        );

        let (started, snapshot) =
            start_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect("start summary job");
        let json = serde_json::to_value(&snapshot).expect("serialize snapshot");

        assert_eq!(json["summaryJob"]["id"], started.id);
        assert_eq!(json["summaryJob"]["state"], "Running");
        assert!(json["analysisCommand"].is_null());
        assert!(json["meetings"][0]["analysis"].is_null());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn summary_job_start_marks_job_failed_when_running_snapshot_cannot_be_built() {
        let root = unique_test_root();
        fs::write(&root, b"not a directory").expect("create invalid app root file");
        let command_state = Mutex::new(DesktopCommandState::default());

        let error =
            start_summary_job_for_app_root(&root, &command_state, "meeting-1", 1_700_000_001_000)
                .expect_err("snapshot creation should fail for invalid app root");
        let job = command_state
            .lock()
            .expect("command state")
            .summary_job
            .clone()
            .expect("failed summary job remains visible");

        assert!(!error.is_empty());
        assert_eq!(job.state, CommandJobState::Failed);
        assert!(!job.cancel_requested);

        fs::remove_file(root).expect("cleanup");
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

    fn audio_manifest_for_test(
        root: &Path,
        recording_id: &str,
        stream: StreamKind,
        relative_path: &str,
        sha256: &str,
    ) -> ArtifactManifest {
        ArtifactManifest {
            recording: RecordingMetadata::new(recording_id, 1_700_000_000_000),
            status: ManifestStatus::Complete,
            ended_at_ms: Some(1_700_000_000_500),
            artifacts: vec![AudioArtifactMetadata {
                stream,
                file_name: relative_path
                    .rsplit('/')
                    .next()
                    .expect("file name")
                    .to_string(),
                path: root.join(relative_path),
                started_at_ms: 1_700_000_000_000,
                ended_at_ms: Some(1_700_000_000_500),
                duration_ms: 500,
                sample_rate_hz: 16_000,
                channel_count: 1,
                identity: DeviceIdentity::new("test", "Test Device", "fixture"),
                bytes_written: 8,
                sha256: sha256.to_string(),
            }],
            recovery: None,
        }
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

    struct StartedFileMicrophoneRecorderFactory;

    impl MicrophoneRecorderFactory for StartedFileMicrophoneRecorderFactory {
        fn start(
            &self,
            audio_root: &Path,
            recording_id: &str,
            started_at_ms: u64,
        ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
            let session_dir = audio_root.join(recording_id);
            fs::create_dir_all(&session_dir).map_err(|error| {
                MicrophoneStartFailure::persistence(format!("create live test audio dir: {error}"))
            })?;
            write_minimal_wav(&session_dir.join("raw-mic.wav"));
            Ok(StartedMicrophoneRecording {
                sample_rate_hz: 48_000,
                streams: vec![StreamKind::Microphone],
                recorder: Box::new(FakeActiveMicrophoneRecording {
                    session_dir,
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

    fn write_truncated_data_chunk_wav(path: &Path) {
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
        fs::write(path, bytes).expect("truncated wav");
    }

    fn write_missing_odd_data_chunk_pad_wav(path: &Path) {
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
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0);
        fs::write(path, bytes).expect("missing odd chunk pad wav");
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

    fn desktop_command_view_contract_fixture() -> serde_json::Value {
        let empty_root = unique_test_root();
        let mut empty_snapshot = serialize_desktop_snapshot_case(&empty_root, |root| {
            desktop_snapshot_for_app_root(root)
        });
        canonicalize_app_root_paths(&mut empty_snapshot, &empty_root);
        fs::remove_dir_all(&empty_root).expect("cleanup empty fixture root");

        let meeting_root = unique_test_root();
        seed_transcribed_analyzed_meeting(&meeting_root);
        let mut meeting_snapshot = serialize_desktop_snapshot_case(&meeting_root, |root| {
            desktop_snapshot_for_app_root(root)
        });
        canonicalize_app_root_paths(&mut meeting_snapshot, &meeting_root);
        fs::remove_dir_all(&meeting_root).expect("cleanup meeting fixture root");

        let evidence_root = unique_test_root();
        fs::create_dir_all(&evidence_root).expect("evidence fixture root");
        let evidence_model_path = evidence_root.join("fixture-whisper.bin");
        fs::write(&evidence_model_path, b"not a real model")
            .expect("evidence fixture whisper model");
        test_whisper_model_path_for_app_root(
            &evidence_root,
            evidence_model_path.to_string_lossy().to_string(),
            1_700_000_001_000,
        )
        .expect("persist evidence fixture whisper test");
        save_whisper_model_path_for_app_root(
            &evidence_root,
            evidence_model_path.to_string_lossy().to_string(),
        )
        .expect("save evidence fixture whisper path");
        test_ollama_connection_for_app_root(
            &evidence_root,
            "http://127.0.0.1:11434".to_string(),
            "qwen3.6:27b".to_string(),
            &RecordingOllamaTransport::tags_response(
                r#"{"models":[{"name":"gemma4:31b"},{"name":"qwen3.6:27b"}]}"#,
            ),
            1_700_000_002_000,
        )
        .expect("persist evidence fixture ollama test");
        save_analysis_settings_for_app_root(
            &evidence_root,
            "http://127.0.0.1:11434".to_string(),
            "qwen3.6:27b".to_string(),
        )
        .expect("save evidence fixture analysis settings");
        let mut evidence_snapshot = serialize_desktop_snapshot_case(&evidence_root, |root| {
            desktop_snapshot_for_app_root(root)
        });
        canonicalize_app_root_paths(&mut evidence_snapshot, &evidence_root);
        fs::remove_dir_all(&evidence_root).expect("cleanup evidence fixture root");

        let unsupported_snapshot_root = unique_test_root();
        fs::create_dir_all(&unsupported_snapshot_root).expect("unsupported snapshot fixture root");
        let unsupported_snapshot_model_path = unsupported_snapshot_root.join("notes.txt");
        fs::write(
            &unsupported_snapshot_model_path,
            b"readable but not a supported model file",
        )
        .expect("unsupported snapshot fixture whisper path");
        save_whisper_model_path_for_app_root(
            &unsupported_snapshot_root,
            unsupported_snapshot_model_path
                .to_string_lossy()
                .to_string(),
        )
        .expect("save unsupported snapshot fixture whisper path");
        let mut unsupported_snapshot =
            serialize_desktop_snapshot_case(&unsupported_snapshot_root, |root| {
                desktop_snapshot_for_app_root(root)
            });
        canonicalize_app_root_paths(&mut unsupported_snapshot, &unsupported_snapshot_root);
        fs::remove_dir_all(&unsupported_snapshot_root)
            .expect("cleanup unsupported snapshot fixture root");

        let whisper_root = unique_test_root();
        fs::create_dir_all(&whisper_root).expect("whisper fixture root");
        let model_path = whisper_root.join("fixture-whisper.bin");
        fs::write(&model_path, b"not a real model").expect("fixture whisper model");
        let readable_whisper = serde_json::to_value(test_whisper_model_path_value(
            model_path.to_string_lossy().as_ref(),
        ))
        .expect("serialize readable whisper path test");
        let unsupported_model_path = whisper_root.join("notes.txt");
        fs::write(
            &unsupported_model_path,
            b"readable but not a supported model file",
        )
        .expect("fixture unsupported whisper path");
        let unsupported_whisper = serde_json::to_value(test_whisper_model_path_value(
            unsupported_model_path.to_string_lossy().as_ref(),
        ))
        .expect("serialize unsupported whisper path test");
        fs::remove_dir_all(&whisper_root).expect("cleanup whisper fixture root");

        let available_ollama = serde_json::to_value(test_ollama_connection_value(
            "http://127.0.0.1:11434",
            "qwen3.6:27b",
            &RecordingOllamaTransport::tags_response(
                r#"{"models":[{"name":"qwen3.6:27b"},{"name":"gemma4:31b"}]}"#,
            ),
        ))
        .expect("serialize available ollama test");
        let missing_ollama = serde_json::to_value(test_ollama_connection_value(
            "http://127.0.0.1:11434",
            "qwen3.6:27b",
            &RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"gemma4:31b"}]}"#),
        ))
        .expect("serialize missing ollama test");
        let cloud_ollama = serde_json::to_value(test_ollama_connection_value(
            "http://127.0.0.1:11434",
            "deepseek-v3.2:cloud",
            &RecordingOllamaTransport::tags_response(r#"{"models":[{"name":"qwen3.6:27b"}]}"#),
        ))
        .expect("serialize cloud ollama test");

        serde_json::json!({
            "version": 1,
            "owner": "apps/desktop/src-tauri/src/main.rs",
            "cases": {
                "desktop_snapshot.empty": empty_snapshot,
                "desktop_snapshot.transcribed_analyzed_meeting": meeting_snapshot,
                "desktop_snapshot.with_setup_evidence": evidence_snapshot,
                "desktop_snapshot.unsupported_whisper_model": unsupported_snapshot,
                "test_whisper_model_path.valid_readable_file": readable_whisper,
                "test_whisper_model_path.unsupported_extension": unsupported_whisper,
                "test_whisper_model_path.missing_path": serde_json::to_value(test_whisper_model_path_value(""))
                    .expect("serialize missing whisper path test"),
                "test_ollama_connection.available_configured_model": available_ollama,
                "test_ollama_connection.missing_local_model": missing_ollama,
                "test_ollama_connection.cloud_model_rejected": cloud_ollama,
            }
        })
    }

    fn serialize_desktop_snapshot_case(
        root: &Path,
        build: impl FnOnce(&Path) -> Result<DesktopSnapshot, String>,
    ) -> serde_json::Value {
        serde_json::to_value(build(root).expect("desktop snapshot fixture case"))
            .expect("serialize desktop snapshot fixture case")
    }

    fn canonicalize_app_root_paths(value: &mut serde_json::Value, app_root: &Path) {
        let app_root = app_root.to_string_lossy().to_string();
        canonicalize_app_root_path_text(value, &app_root);
    }

    fn canonicalize_app_root_path_text(value: &mut serde_json::Value, app_root: &str) {
        match value {
            serde_json::Value::String(text) => {
                if text.contains(app_root) {
                    *text = text.replace(app_root, "<app-root>");
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    canonicalize_app_root_path_text(item, app_root);
                }
            }
            serde_json::Value::Object(fields) => {
                for item in fields.values_mut() {
                    canonicalize_app_root_path_text(item, app_root);
                }
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            }
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

    struct EnvVarRestoreGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarRestoreGuard {
        fn capture(key: &'static str) -> Self {
            Self {
                key,
                previous: std::env::var(key).ok(),
            }
        }

        fn unset(key: &'static str) -> Self {
            let guard = Self::capture(key);
            std::env::remove_var(key);
            guard
        }
    }

    impl Drop for EnvVarRestoreGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
