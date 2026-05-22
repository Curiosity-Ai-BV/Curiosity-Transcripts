use std::path::{Path, PathBuf};
use std::sync::Mutex;

use curiosity_app::{
    list_meetings_dto, meeting_detail_dto, AppPermissionState, CommandRecordingDto,
    CommandRecordingState, RawAudioRetentionPolicy, StorageLocationDto,
};
use curiosity_audio::{
    ArtifactManifest, CaptureError, CapturePermission, MacosMicrophoneWavRecording,
    ManualSmokeCheck, ManualSmokeResult, ManualSmokeStatus, ScreenCaptureKitSystemAudioAdapter,
    StreamKind, SystemAudioAdapterStatus,
};
use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, MeetingStatus, ModelRun, RecordingSession,
    RecordingSource, RecordingStatus, SourceChannel, TranscriptVersion,
};
use curiosity_store::{AppSettings, Store};
#[cfg(feature = "whisper-rs")]
use curiosity_transcription::RealWhisperBackend;
use curiosity_transcription::{
    TranscriptionDocument, TranscriptionError, WhisperBackend, WhisperTranscriber,
    WhisperTranscriptionRequest,
};
use serde::Serialize;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(DesktopCommandState::default()))
        .invoke_handler(tauri::generate_handler![
            desktop_snapshot,
            get_settings,
            save_whisper_model_path,
            save_analysis_settings,
            test_whisper_model_path,
            audio_smoke_status,
            system_audio_smoke_recording,
            start_microphone_recording,
            stop_microphone_recording,
            transcribe_meeting
        ])
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
            .ok_or_else(|| "Start a microphone recording before stopping.".to_string())?
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
    let settings = store
        .app_settings()
        .map_err(|error| error.to_string())?;
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
        recording: recording_snapshot(app_root, command_state),
        model: model_status_from_settings(&settings),
        settings: app_settings_view(settings),
        capture: CaptureStatus {
            microphone: microphone_capture_state(command_state),
            system_audio: DesktopPermissionState::SystemAudioUnavailable,
        },
        transcription: command_state.last_transcription.clone(),
    })
}

fn open_store(app_root: &Path) -> Result<Store, String> {
    std::fs::create_dir_all(app_root).map_err(|error| error.to_string())?;
    let store = Store::open(app_root.join("curiosity.sqlite3"), app_root.to_path_buf())
        .map_err(|error| error.to_string())?;
    store.migrate().map_err(|error| error.to_string())?;
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

#[derive(Default)]
struct DesktopCommandState {
    active_recording: Option<ActiveDesktopRecording>,
    starting_recording: bool,
    last_recording: Option<CommandRecordingDto>,
    last_transcription: Option<TranscriptionCommandView>,
}

impl DesktopCommandState {
    fn snapshot_state(&self) -> DesktopCommandSnapshotState {
        DesktopCommandSnapshotState {
            active_recording: self.active_recording.as_ref().map(|recording| {
                ActiveDesktopRecordingSnapshot {
                    meeting_id: recording.meeting_id.clone(),
                    recording_id: recording.recording_id.clone(),
                }
            }),
            last_recording: self.last_recording.clone(),
            last_transcription: self.last_transcription.clone(),
        }
    }
}

#[derive(Clone, Default)]
struct DesktopCommandSnapshotState {
    active_recording: Option<ActiveDesktopRecordingSnapshot>,
    last_recording: Option<CommandRecordingDto>,
    last_transcription: Option<TranscriptionCommandView>,
}

#[derive(Clone)]
struct ActiveDesktopRecordingSnapshot {
    meeting_id: String,
    recording_id: String,
}

struct ActiveDesktopRecording {
    meeting_id: String,
    recording_id: String,
    recorder: Box<dyn ActiveMicrophoneRecording>,
}

struct StartedMicrophoneRecording {
    sample_rate_hz: u32,
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
                let guidance = error.recovery_guidance();
                Self {
                    permission_state: AppPermissionState::MicrophoneUnavailable,
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
        let recorder = MacosMicrophoneWavRecording::start(audio_root, recording_id, started_at_ms)
            .map_err(MicrophoneStartFailure::from_capture_error)?;
        let sample_rate_hz = recorder.sample_rate_hz();
        Ok(StartedMicrophoneRecording {
            sample_rate_hz,
            recorder: Box::new(recorder),
        })
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
        recorder,
    } = factory.start(&audio_root, &recording_id, started_at_ms)?;

    let mut meeting = Meeting::new_manual(&meeting_id, title, started_at_ms);
    let session = RecordingSession::start(
        &recording_id,
        &meeting_id,
        RecordingSource::Microphone,
        started_at_ms,
        sample_rate_hz,
    );
    meeting.start_recording(&session);
    let artifact = AudioArtifact::new_private(
        artifact_id(&recording_id),
        &recording_id,
        ArtifactKind::RawMic,
        microphone_artifact_relative_path(&meeting_id, &recording_id),
        format!("sha256:pending:{}", artifact_id(&recording_id)),
    );

    if let Err(error) = store.insert_recording_start(&meeting, &session, &artifact) {
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
        return Err("Start a microphone recording before stopping.".to_string());
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
                AppPermissionState::MicrophoneUnavailable,
                microphone_storage_path(&meeting_id),
                &format!("Recording could not be finalized: {message}"),
            )
        }
    }
}

fn complete_active_microphone_recording(
    app_root: &Path,
    active: ActiveDesktopRecording,
    ended_at_ms: u64,
) -> Result<CommandRecordingDto, String> {
    let manifest = active.recorder.stop(ended_at_ms)?;
    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.stream == StreamKind::Microphone)
        .ok_or_else(|| "microphone recording stopped without a WAV artifact".to_string())?;
    let relative_path = artifact
        .path
        .strip_prefix(app_root)
        .map_err(|_| "microphone artifact was written outside private app storage".to_string())?
        .to_string_lossy()
        .to_string();
    let expected_path = microphone_artifact_relative_path(&active.meeting_id, &active.recording_id);
    if relative_path != expected_path {
        return Err(format!(
            "microphone artifact path mismatch: expected {expected_path}, got {relative_path}"
        ));
    }

    let store = open_store(app_root)?;
    store
        .complete_audio_artifact(&artifact_id(&active.recording_id), &artifact.sha256)
        .map_err(|error| error.to_string())?;
    store
        .update_recording_session_status(
            &active.recording_id,
            RecordingStatus::Complete,
            Some(ended_at_ms),
            None,
        )
        .map_err(|error| error.to_string())?;
    store
        .update_meeting_status(
            &active.meeting_id,
            MeetingStatus::Complete,
            Some(ended_at_ms),
        )
        .map_err(|error| error.to_string())?;

    Ok(recording_dto(
        &active.meeting_id,
        Some(active.recording_id),
        CommandRecordingState::Complete,
        AppPermissionState::Ready,
        microphone_storage_path(&active.meeting_id),
        "Finalized local microphone WAV artifact.",
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
    let Some(artifact) = store
        .completed_wav_artifact_for_transcription(meeting_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(transcription_failed(
            meeting_id,
            "missing_audio_artifact",
            "No completed retained local WAV artifact exists for this meeting.",
            "Stop a microphone recording before requesting transcription.",
        ));
    };

    let source_channel = source_channel_for_artifact_kind(&artifact.kind);
    let request = WhisperTranscriptionRequest::new(
        meeting_id,
        app_root.join(&artifact.path),
        artifact.sha256.clone(),
        source_channel,
    );
    let transcriber = WhisperTranscriber::new(model_path, model_name, backend);
    match transcriber.transcribe_wav(&request) {
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
struct BackendUnavailableWhisperBackend;

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
        CommandRecordingState::Interrupted,
        AppPermissionState::MicrophoneUnavailable,
        app_root.display().to_string(),
        "Start a microphone recording to create a private WAV artifact.",
    )
}

fn microphone_capture_state(command_state: &DesktopCommandSnapshotState) -> DesktopPermissionState {
    if command_state.active_recording.is_some() {
        return DesktopPermissionState::Ready;
    }
    if command_state
        .last_recording
        .as_ref()
        .map(|recording| recording.permission_state == AppPermissionState::MicrophoneDenied)
        .unwrap_or(false)
    {
        return DesktopPermissionState::MicrophoneDenied;
    }
    DesktopPermissionState::MicrophoneUnavailable
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
            "Microphone recording could not start: {} {}",
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

fn microphone_storage_path(meeting_id: &str) -> String {
    format!("meetings/{meeting_id}/audio")
}

fn microphone_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/raw-mic.wav",
        microphone_storage_path(meeting_id)
    )
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
            setup_guidance: "Save this path, then transcribe with the whisper-rs desktop feature enabled.".to_string(),
        },
        Err(error) => WhisperModelPathTestView::invalid(
            format!("Whisper model path is not readable: {error}"),
            "Check file permissions and choose a readable local Whisper model file.",
        ),
    }
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
    use curiosity_audio::{
        ArtifactManifest, AudioArtifactMetadata, DeviceIdentity, ManifestStatus, RecordingMetadata,
        StreamKind,
    };
    use curiosity_domain::{
        AnalysisCitation, Meeting, MeetingAnalysis, ModelRun, SourceChannel, TranscriptSegment,
        TranscriptVersion,
    };
    use curiosity_transcription::{FakeWhisperBackend, WhisperBackendSegment};
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
        assert_eq!(
            json["recording"]["permission_state"],
            "MicrophoneUnavailable"
        );
        assert_eq!(
            json["recording"]["recovery_action"],
            "Start a microphone recording to create a private WAV artifact."
        );
        assert_eq!(
            json["recording"]["storage_location"]["app_private_path"],
            root.display().to_string()
        );
        assert_eq!(json["model"]["kind"], "missing");
        assert_eq!(json["capture"]["microphone"], "MicrophoneUnavailable");
        assert_eq!(json["capture"]["systemAudio"], "SystemAudioUnavailable");
        assert_eq!(
            json["settings"]["ollamaBaseUrl"],
            "http://127.0.0.1:11434"
        );
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
        save_whisper_model_path_for_app_root(
            &root,
            saved_model_path.to_string_lossy().to_string(),
        )
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
    fn start_microphone_recording_with_fake_recorder_returns_active_snapshot() {
        let root = unique_test_root();
        let mut command_state = DesktopCommandState::default();
        let factory = FakeMicrophoneRecorderFactory::default();

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
        let factory = FakeMicrophoneRecorderFactory::default();
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
        assert_eq!(json["capture"]["microphone"], "MicrophoneUnavailable");
        assert_eq!(json["meetings"][0]["status"], "Complete");
        assert_eq!(artifact.sha256.len(), 64);
        assert!(root.join(&artifact.path).is_file());

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
        let factory = FakeMicrophoneRecorderFactory::default();
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
