use curiosity_audio::{measure_drift, AudioFrame, StreamKind};

fn frame(stream: StreamKind, start_time_ms: u64, sample_count: usize) -> AudioFrame {
    frame_with_channels(stream, start_time_ms, sample_count, 1)
}

fn frame_with_channels(
    stream: StreamKind,
    start_time_ms: u64,
    sample_count: usize,
    channel_count: u16,
) -> AudioFrame {
    AudioFrame {
        stream,
        start_time_ms,
        sample_rate_hz: 48_000,
        channel_count,
        pcm_i16: vec![0; sample_count],
    }
}

#[test]
fn drift_measurement_uses_deterministic_timestamps_and_sample_counts() {
    let mic = vec![
        frame(StreamKind::Microphone, 1_000, 48_000),
        frame(StreamKind::Microphone, 2_000, 48_000),
    ];
    let system = vec![
        frame(StreamKind::SystemAudio, 1_003, 48_010),
        frame(StreamKind::SystemAudio, 2_004, 48_010),
    ];

    let drift = measure_drift(&mic, &system).expect("drift");

    assert_eq!(drift.mic_duration_ms, 2_000);
    assert_eq!(drift.system_duration_ms, 2_000);
    assert_eq!(drift.timestamp_delta_ms, 4);
    assert_eq!(drift.sample_count_delta, 20);
    assert_eq!(drift.sample_rate_hz, 48_000);
}

#[test]
fn drift_measurement_fails_loud_when_streams_are_missing() {
    let mic = vec![frame(StreamKind::Microphone, 1_000, 48_000)];
    let system = Vec::new();

    let error = measure_drift(&mic, &system).expect_err("missing stream should fail");

    assert!(error.contains("system"));
}

#[test]
fn drift_measurement_treats_pcm_as_interleaved_for_stereo_duration() {
    let mic = vec![frame_with_channels(
        StreamKind::Microphone,
        1_000,
        96_000,
        2,
    )];
    let system = vec![frame_with_channels(
        StreamKind::SystemAudio,
        1_000,
        96_000,
        2,
    )];

    let drift = measure_drift(&mic, &system).expect("stereo drift");

    assert_eq!(drift.mic_duration_ms, 1_000);
    assert_eq!(drift.system_duration_ms, 1_000);
    assert_eq!(drift.sample_count_delta, 0);
    assert_eq!(drift.timestamp_delta_ms, 0);
}
