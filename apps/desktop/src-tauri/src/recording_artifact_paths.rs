use curiosity_audio::StreamKind;

pub(super) fn artifact_id(recording_id: &str) -> String {
    format!("artifact-{recording_id}")
}

pub(super) fn system_audio_artifact_id(recording_id: &str) -> String {
    format!("artifact-{recording_id}-system")
}

pub(super) fn imported_artifact_id(recording_id: &str) -> String {
    format!("artifact-{recording_id}-imported")
}

pub(super) fn artifact_id_for_stream(recording_id: &str, stream: StreamKind) -> String {
    match stream {
        StreamKind::Microphone => artifact_id(recording_id),
        StreamKind::SystemAudio => system_audio_artifact_id(recording_id),
    }
}

pub(super) fn microphone_storage_path(meeting_id: &str) -> String {
    format!("meetings/{meeting_id}/audio")
}

pub(super) fn microphone_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/raw-mic.wav",
        microphone_storage_path(meeting_id)
    )
}

pub(super) fn system_audio_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/raw-system.wav",
        microphone_storage_path(meeting_id)
    )
}

pub(super) fn imported_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/imported.wav",
        microphone_storage_path(meeting_id)
    )
}

pub(super) fn imported_temp_artifact_relative_path(meeting_id: &str, recording_id: &str) -> String {
    format!(
        "{}/{recording_id}/imported.wav.tmp",
        microphone_storage_path(meeting_id)
    )
}

pub(super) fn artifact_relative_path_for_stream(
    meeting_id: &str,
    recording_id: &str,
    stream: StreamKind,
) -> String {
    match stream {
        StreamKind::Microphone => microphone_artifact_relative_path(meeting_id, recording_id),
        StreamKind::SystemAudio => system_audio_artifact_relative_path(meeting_id, recording_id),
    }
}
