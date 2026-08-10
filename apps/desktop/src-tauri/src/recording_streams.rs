use curiosity_audio::StreamKind;
use curiosity_domain::RecordingSource;

pub(super) fn stream_label(stream: StreamKind) -> &'static str {
    match stream {
        StreamKind::Microphone => "microphone",
        StreamKind::SystemAudio => "system audio",
    }
}

pub(super) fn recording_source_for_streams(streams: &[StreamKind]) -> RecordingSource {
    let has_microphone = streams.contains(&StreamKind::Microphone);
    let has_system_audio = streams.contains(&StreamKind::SystemAudio);
    match (has_microphone, has_system_audio) {
        (true, true) => RecordingSource::Mixed,
        (false, true) => RecordingSource::System,
        _ => RecordingSource::Microphone,
    }
}

pub(super) fn required_recording_source_for_streams(streams: &[StreamKind]) -> RecordingSource {
    if streams.contains(&StreamKind::Microphone) {
        RecordingSource::Microphone
    } else {
        recording_source_for_streams(streams)
    }
}
