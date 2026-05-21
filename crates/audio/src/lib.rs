use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamKind {
    Microphone,
    SystemAudio,
}

impl StreamKind {
    pub fn as_manifest_str(self) -> &'static str {
        match self {
            StreamKind::Microphone => "Microphone",
            StreamKind::SystemAudio => "SystemAudio",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceIdentity {
    pub identity: String,
    pub display_name: String,
    pub transport: String,
}

impl DeviceIdentity {
    pub fn new(identity: &str, display_name: &str, transport: &str) -> Self {
        Self {
            identity: identity.to_string(),
            display_name: display_name.to_string(),
            transport: transport.to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamMetadata {
    pub stream: StreamKind,
    pub sample_rate_hz: u32,
    pub channel_count: u16,
    pub identity: DeviceIdentity,
    pub start_time_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    pub captured_at_ms: u64,
    pub mic: Option<StreamMetadata>,
    pub system: Option<StreamMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioFrame {
    pub stream: StreamKind,
    pub start_time_ms: u64,
    pub sample_rate_hz: u32,
    pub channel_count: u16,
    pub pcm_i16: Vec<i16>,
}

pub trait AudioCapture {
    fn device_snapshot(&self) -> Result<DeviceSnapshot, CapturePermissionError>;
    fn capture_frames(&self) -> Result<Vec<AudioFrame>, CapturePermissionError>;
}

pub struct FakeAudioCapture {
    mic: DeviceIdentity,
    system: DeviceIdentity,
    sample_rate_hz: u32,
    channel_count: u16,
    start_time_ms: u64,
}

impl FakeAudioCapture {
    pub fn new_deterministic(sample_rate_hz: u32, channel_count: u16, start_time_ms: u64) -> Self {
        Self::with_devices(
            DeviceIdentity::new("fake-mic", "Fake Microphone", "test"),
            DeviceIdentity::new("fake-system", "Fake System Audio", "test"),
            sample_rate_hz,
            channel_count,
            start_time_ms,
        )
    }

    pub fn with_devices(
        mic: DeviceIdentity,
        system: DeviceIdentity,
        sample_rate_hz: u32,
        channel_count: u16,
        start_time_ms: u64,
    ) -> Self {
        Self {
            mic,
            system,
            sample_rate_hz,
            channel_count,
            start_time_ms,
        }
    }
}

impl AudioCapture for FakeAudioCapture {
    fn device_snapshot(&self) -> Result<DeviceSnapshot, CapturePermissionError> {
        Ok(DeviceSnapshot {
            captured_at_ms: self.start_time_ms,
            mic: Some(StreamMetadata {
                stream: StreamKind::Microphone,
                sample_rate_hz: self.sample_rate_hz,
                channel_count: self.channel_count,
                identity: self.mic.clone(),
                start_time_ms: self.start_time_ms,
            }),
            system: Some(StreamMetadata {
                stream: StreamKind::SystemAudio,
                sample_rate_hz: self.sample_rate_hz,
                channel_count: self.channel_count,
                identity: self.system.clone(),
                start_time_ms: self.start_time_ms,
            }),
        })
    }

    fn capture_frames(&self) -> Result<Vec<AudioFrame>, CapturePermissionError> {
        Ok(vec![
            AudioFrame {
                stream: StreamKind::Microphone,
                start_time_ms: self.start_time_ms,
                sample_rate_hz: self.sample_rate_hz,
                channel_count: self.channel_count,
                pcm_i16: vec![0, 1000, 0, -1000],
            },
            AudioFrame {
                stream: StreamKind::SystemAudio,
                start_time_ms: self.start_time_ms,
                sample_rate_hz: self.sample_rate_hz,
                channel_count: self.channel_count,
                pcm_i16: vec![500, 0, -500, 0],
            },
        ])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePermission {
    Microphone,
    SystemAudioScreenRecording,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturePermissionError {
    pub permission: CapturePermission,
    pub message: String,
}

impl CapturePermissionError {
    pub fn denied(permission: CapturePermission) -> Self {
        let message = match permission {
            CapturePermission::Microphone => "Microphone permission is denied",
            CapturePermission::SystemAudioScreenRecording => {
                "Screen Recording permission is denied for system audio capture"
            }
        };
        Self {
            permission,
            message: message.to_string(),
        }
    }

    pub fn recovery_guidance(&self) -> UserRecoveryGuidance {
        match self.permission {
            CapturePermission::Microphone => UserRecoveryGuidance {
                title: "Microphone permission required".to_string(),
                steps: vec![
                    "Open System Settings".to_string(),
                    "Go to Privacy & Security, then Microphone".to_string(),
                    "Allow Curiosity Transcripts and restart recording".to_string(),
                ],
            },
            CapturePermission::SystemAudioScreenRecording => UserRecoveryGuidance {
                title: "Screen Recording permission required".to_string(),
                steps: vec![
                    "Open System Settings".to_string(),
                    "Go to Privacy & Security, then Screen Recording".to_string(),
                    "Allow Curiosity Transcripts, then restart the app before recording system audio"
                        .to_string(),
                ],
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserRecoveryGuidance {
    pub title: String,
    pub steps: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualSmokeStatus {
    NotRun,
    Skipped,
    Unavailable,
    PermissionDenied,
    Passed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualSmokeResult {
    pub status: ManualSmokeStatus,
    pub message: String,
}

pub struct ManualSmokeCheck;

impl ManualSmokeCheck {
    pub fn macos_placeholder() -> Self {
        Self
    }

    pub fn run_without_hardware(&self) -> ManualSmokeResult {
        ManualSmokeResult {
            status: ManualSmokeStatus::NotRun,
            message: "macOS audio capture smoke is manual until hardware capture is wired"
                .to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingMetadata {
    pub session_id: String,
    pub started_at_ms: u64,
}

impl RecordingMetadata {
    pub fn new(session_id: &str, started_at_ms: u64) -> Self {
        Self {
            session_id: session_id.to_string(),
            started_at_ms,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestStatus {
    Recording,
    Complete,
    Canceled,
    Failed,
}

impl ManifestStatus {
    pub fn as_manifest_str(self) -> &'static str {
        match self {
            ManifestStatus::Recording => "Recording",
            ManifestStatus::Complete => "Complete",
            ManifestStatus::Canceled => "Canceled",
            ManifestStatus::Failed => "Failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkStatus {
    Writing,
    Complete,
    Interrupted,
}

impl ChunkStatus {
    pub fn as_manifest_str(self) -> &'static str {
        match self {
            ChunkStatus::Writing => "Writing",
            ChunkStatus::Complete => "Complete",
            ChunkStatus::Interrupted => "Interrupted",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkRecoveryState {
    NotNeeded,
    RecoverableInterrupted,
    NotRecoverable,
}

impl ChunkRecoveryState {
    pub fn as_manifest_str(self) -> &'static str {
        match self {
            ChunkRecoveryState::NotNeeded => "NotNeeded",
            ChunkRecoveryState::RecoverableInterrupted => "RecoverableInterrupted",
            ChunkRecoveryState::NotRecoverable => "NotRecoverable",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkMetadata {
    pub stream: StreamKind,
    pub path: PathBuf,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub sample_rate_hz: u32,
    pub channel_count: u16,
    pub bytes_written: u64,
    pub status: ChunkStatus,
    pub recovery: ChunkRecoveryState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryMetadata {
    pub recoverable: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkManifest {
    pub recording: RecordingMetadata,
    pub status: ManifestStatus,
    pub ended_at_ms: Option<u64>,
    pub chunks: Vec<ChunkMetadata>,
    pub recovery: Option<RecoveryMetadata>,
}

pub struct ChunkWriter {
    session_dir: PathBuf,
    manifest: ChunkManifest,
}

impl ChunkWriter {
    pub fn create(root: &Path, recording: RecordingMetadata) -> io::Result<Self> {
        let session_dir = root.join(&recording.session_id);
        fs::create_dir_all(&session_dir)?;
        let writer = Self {
            session_dir,
            manifest: ChunkManifest {
                recording,
                status: ManifestStatus::Recording,
                ended_at_ms: None,
                chunks: Vec::new(),
                recovery: None,
            },
        };
        writer.write_manifest()?;
        Ok(writer)
    }

    pub fn write_frame(&mut self, frame: &AudioFrame) -> io::Result<()> {
        let file_name = match frame.stream {
            StreamKind::Microphone => "raw-mic.pcm",
            StreamKind::SystemAudio => "raw-system.pcm",
        };
        let path = self.session_dir.join(file_name);
        let mut file = File::options().create(true).append(true).open(&path)?;
        for sample in &frame.pcm_i16 {
            file.write_all(&sample.to_le_bytes())?;
        }
        file.sync_data()?;

        let bytes_written = (frame.pcm_i16.len() * std::mem::size_of::<i16>()) as u64;
        let frame_end_time_ms = frame_end_time_ms(frame);
        if let Some(chunk) = self
            .manifest
            .chunks
            .iter_mut()
            .find(|chunk| chunk.stream == frame.stream)
        {
            chunk.bytes_written += bytes_written;
            chunk.ended_at_ms = Some(frame_end_time_ms);
        } else {
            self.manifest.chunks.push(ChunkMetadata {
                stream: frame.stream,
                path,
                started_at_ms: frame.start_time_ms,
                ended_at_ms: Some(frame_end_time_ms),
                sample_rate_hz: frame.sample_rate_hz,
                channel_count: frame.channel_count,
                bytes_written,
                status: ChunkStatus::Writing,
                recovery: ChunkRecoveryState::NotNeeded,
            });
        }
        self.write_manifest()
    }

    pub fn stop(mut self, ended_at_ms: u64) -> io::Result<ChunkManifest> {
        self.manifest.status = ManifestStatus::Complete;
        self.manifest.ended_at_ms = Some(ended_at_ms);
        for chunk in &mut self.manifest.chunks {
            chunk.status = ChunkStatus::Complete;
            chunk.recovery = ChunkRecoveryState::NotNeeded;
        }
        self.write_manifest()?;
        Ok(self.manifest)
    }

    pub fn cancel(mut self, ended_at_ms: u64, reason: &str) -> io::Result<ChunkManifest> {
        self.finish_interrupted(ManifestStatus::Canceled, ended_at_ms, reason)
    }

    pub fn fail(mut self, ended_at_ms: u64, reason: &str) -> io::Result<ChunkManifest> {
        self.finish_interrupted(ManifestStatus::Failed, ended_at_ms, reason)
    }

    fn finish_interrupted(
        &mut self,
        status: ManifestStatus,
        ended_at_ms: u64,
        reason: &str,
    ) -> io::Result<ChunkManifest> {
        let recoverable = self
            .manifest
            .chunks
            .iter()
            .any(|chunk| chunk.bytes_written > 0);
        self.manifest.status = status;
        self.manifest.ended_at_ms = Some(ended_at_ms);
        self.manifest.recovery = Some(RecoveryMetadata {
            recoverable,
            reason: reason.to_string(),
        });
        for chunk in &mut self.manifest.chunks {
            chunk.status = ChunkStatus::Interrupted;
            chunk.recovery = if chunk.bytes_written > 0 {
                ChunkRecoveryState::RecoverableInterrupted
            } else {
                ChunkRecoveryState::NotRecoverable
            };
        }
        self.write_manifest()?;
        Ok(self.manifest.clone())
    }

    fn write_manifest(&self) -> io::Result<()> {
        let path = self.session_dir.join("manifest.txt");
        let mut file = File::create(path)?;
        writeln!(file, "session_id={}", self.manifest.recording.session_id)?;
        writeln!(file, "status={}", self.manifest.status.as_manifest_str())?;
        writeln!(file, "started_at_ms={}", self.manifest.recording.started_at_ms)?;
        writeln!(file, "ended_at_ms={:?}", self.manifest.ended_at_ms)?;
        if let Some(recovery) = &self.manifest.recovery {
            writeln!(file, "recoverable={}", recovery.recoverable)?;
            writeln!(file, "recovery_reason={}", recovery.reason)?;
        }
        for chunk in &self.manifest.chunks {
            writeln!(
                file,
                "chunk={},{},{},{}",
                chunk.stream.as_manifest_str(),
                chunk.status.as_manifest_str(),
                chunk.recovery.as_manifest_str(),
                chunk.bytes_written
            )?;
        }
        file.sync_data()
    }
}

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

fn frame_end_time_ms(frame: &AudioFrame) -> u64 {
    if frame.channel_count == 0 || frame.sample_rate_hz == 0 || frame.pcm_i16.is_empty() {
        return frame.start_time_ms;
    }
    let audio_frames = frame.pcm_i16.len() as u64 / frame.channel_count as u64;
    let duration_ms = audio_frames.saturating_mul(1_000).div_ceil(frame.sample_rate_hz as u64);
    frame.start_time_ms.saturating_add(duration_ms)
}

fn samples_to_ms(samples: usize, sample_rate_hz: u32) -> u64 {
    ((samples as u64) * 1_000) / sample_rate_hz as u64
}
