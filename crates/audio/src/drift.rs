use crate::AudioFrame;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriftMeasurement {
    pub mic_duration_ms: u64,
    pub system_duration_ms: u64,
    pub timestamp_delta_ms: i64,
    pub sample_count_delta: i64,
    pub sample_rate_hz: u32,
}

pub fn measure_drift(
    mic_frames: &[AudioFrame],
    system_frames: &[AudioFrame],
) -> Result<DriftMeasurement, String> {
    if mic_frames.is_empty() {
        return Err("missing microphone frames".to_string());
    }
    if system_frames.is_empty() {
        return Err("missing system frames".to_string());
    }

    let mic_sample_rate = mic_frames[0].sample_rate_hz;
    let system_sample_rate = system_frames[0].sample_rate_hz;
    if mic_sample_rate != system_sample_rate {
        return Err("sample rates must match before drift comparison".to_string());
    }

    let mic_samples = total_interleaved_frames(mic_frames)?;
    let system_samples = total_interleaved_frames(system_frames)?;
    let mic_duration_ms = samples_to_ms(mic_samples, mic_sample_rate);
    let system_duration_ms = samples_to_ms(system_samples, system_sample_rate);
    let mic_end = mic_frames
        .last()
        .expect("checked")
        .start_time_ms
        .saturating_add(frame_duration_ms(mic_frames.last().expect("checked"))?);
    let system_end = system_frames
        .last()
        .expect("checked")
        .start_time_ms
        .saturating_add(frame_duration_ms(system_frames.last().expect("checked"))?);

    Ok(DriftMeasurement {
        mic_duration_ms,
        system_duration_ms,
        timestamp_delta_ms: system_end as i64 - mic_end as i64,
        sample_count_delta: system_samples as i64 - mic_samples as i64,
        sample_rate_hz: mic_sample_rate,
    })
}

fn total_interleaved_frames(frames: &[AudioFrame]) -> Result<usize, String> {
    frames.iter().try_fold(0usize, |total, frame| {
        audio_frame_count(frame).map(|count| total + count)
    })
}

fn audio_frame_count(frame: &AudioFrame) -> Result<usize, String> {
    if frame.channel_count == 0 {
        return Err("channel count must be greater than zero".to_string());
    }
    Ok(frame.pcm_i16.len() / frame.channel_count as usize)
}

fn frame_duration_ms(frame: &AudioFrame) -> Result<u64, String> {
    Ok(samples_to_ms(
        audio_frame_count(frame)?,
        frame.sample_rate_hz,
    ))
}

fn samples_to_ms(samples: usize, sample_rate_hz: u32) -> u64 {
    ((samples as u64) * 1_000) / sample_rate_hz as u64
}
