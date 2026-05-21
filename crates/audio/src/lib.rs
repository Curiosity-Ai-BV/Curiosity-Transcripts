use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
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
pub struct CaptureConfiguration {
    microphone: bool,
    system_audio: bool,
}

impl CaptureConfiguration {
    pub fn new(microphone: bool, system_audio: bool) -> Result<Self, CaptureConfigurationError> {
        if !microphone && !system_audio {
            return Err(CaptureConfigurationError::NoStreamsRequested);
        }
        Ok(Self {
            microphone,
            system_audio,
        })
    }

    pub fn mic_only() -> Result<Self, CaptureConfigurationError> {
        Self::new(true, false)
    }

    pub fn system_only() -> Result<Self, CaptureConfigurationError> {
        Self::new(false, true)
    }

    pub fn mixed() -> Result<Self, CaptureConfigurationError> {
        Self::new(true, true)
    }

    pub fn requests(&self, stream: StreamKind) -> bool {
        match stream {
            StreamKind::Microphone => self.microphone,
            StreamKind::SystemAudio => self.system_audio,
        }
    }

    pub fn requested_streams(&self) -> Vec<StreamKind> {
        let mut streams = Vec::new();
        if self.microphone {
            streams.push(StreamKind::Microphone);
        }
        if self.system_audio {
            streams.push(StreamKind::SystemAudio);
        }
        streams
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureConfigurationError {
    NoStreamsRequested,
}

impl fmt::Display for CaptureConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureConfigurationError::NoStreamsRequested => {
                write!(f, "capture requires at least one requested audio stream")
            }
        }
    }
}

impl Error for CaptureConfigurationError {}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureCapability {
    Microphone,
    SystemAudio,
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

impl fmt::Display for CapturePermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for CapturePermissionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureUnavailable {
    pub capability: CaptureCapability,
    pub reason: String,
}

impl CaptureUnavailable {
    pub fn microphone(reason: impl Into<String>) -> Self {
        Self {
            capability: CaptureCapability::Microphone,
            reason: reason.into(),
        }
    }

    pub fn system_audio(reason: impl Into<String>) -> Self {
        Self {
            capability: CaptureCapability::SystemAudio,
            reason: reason.into(),
        }
    }

    pub fn recovery_guidance(&self) -> UserRecoveryGuidance {
        match self.capability {
            CaptureCapability::Microphone => UserRecoveryGuidance {
                title: "Microphone unavailable".to_string(),
                steps: vec![
                    "Connect or select a macOS input device".to_string(),
                    "Open System Settings, then Sound, then Input to confirm the device is visible"
                        .to_string(),
                    "If the device is visible but capture still fails, check Privacy & Security, then Microphone"
                        .to_string(),
                ],
            },
            CaptureCapability::SystemAudio => UserRecoveryGuidance {
                title: "System audio unavailable".to_string(),
                steps: vec![
                    "System audio capture requires a macOS ScreenCaptureKit adapter".to_string(),
                    "Open System Settings, then Privacy & Security, then Screen Recording"
                        .to_string(),
                    "Allow Curiosity Transcripts and restart the app before recording system audio"
                        .to_string(),
                ],
            },
        }
    }
}

impl fmt::Display for CaptureUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.capability {
            CaptureCapability::Microphone => write!(f, "microphone unavailable: {}", self.reason),
            CaptureCapability::SystemAudio => {
                write!(f, "system audio unavailable: {}", self.reason)
            }
        }
    }
}

impl Error for CaptureUnavailable {}

#[derive(Debug)]
pub enum CaptureError {
    Configuration(CaptureConfigurationError),
    PermissionDenied(CapturePermissionError),
    Unavailable(CaptureUnavailable),
    Recording(RecordingError),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Configuration(error) => write!(f, "{error}"),
            CaptureError::PermissionDenied(error) => write!(f, "{error}"),
            CaptureError::Unavailable(error) => write!(f, "{error}"),
            CaptureError::Recording(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CaptureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CaptureError::Configuration(error) => Some(error),
            CaptureError::PermissionDenied(error) => Some(error),
            CaptureError::Unavailable(error) => Some(error),
            CaptureError::Recording(error) => Some(error),
        }
    }
}

impl From<CaptureConfigurationError> for CaptureError {
    fn from(error: CaptureConfigurationError) -> Self {
        CaptureError::Configuration(error)
    }
}

impl From<RecordingError> for CaptureError {
    fn from(error: RecordingError) -> Self {
        CaptureError::Recording(error)
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
            status: ManualSmokeStatus::Skipped,
            message: "macOS audio smoke skipped; rerun audio-smoke with --attempt-mic to request microphone hardware capture"
                .to_string(),
        }
    }

    pub fn run_macos_microphone_capture(
        &self,
        root: &Path,
        duration: Duration,
    ) -> ManualSmokeResult {
        match record_macos_microphone_to_wav(root, duration) {
            Ok(manifest) => ManualSmokeResult::from_artifact_manifest(&manifest),
            Err(error) => ManualSmokeResult::from_capture_error(error),
        }
    }

    pub fn run_macos_system_audio_capture(
        &self,
        root: &Path,
        duration: Duration,
    ) -> ManualSmokeResult {
        match record_macos_system_audio_to_wav(root, duration) {
            Ok(manifest) => ManualSmokeResult::from_artifact_manifest(&manifest),
            Err(error) => ManualSmokeResult::from_capture_error(error),
        }
    }
}

impl ManualSmokeResult {
    pub fn from_capture_error(error: CaptureError) -> Self {
        match error {
            CaptureError::PermissionDenied(error) => {
                let guidance = error.recovery_guidance();
                Self {
                    status: ManualSmokeStatus::PermissionDenied,
                    message: format!("{}: {}", guidance.title, guidance.steps.join("; ")),
                }
            }
            CaptureError::Unavailable(error) => {
                let guidance = error.recovery_guidance();
                Self {
                    status: ManualSmokeStatus::Unavailable,
                    message: format!("{}: {}", error, guidance.steps.join("; ")),
                }
            }
            CaptureError::Configuration(error) => Self {
                status: ManualSmokeStatus::Unavailable,
                message: error.to_string(),
            },
            CaptureError::Recording(error) => Self {
                status: ManualSmokeStatus::Unavailable,
                message: error.to_string(),
            },
        }
    }

    pub fn from_artifact_manifest(manifest: &ArtifactManifest) -> Self {
        let Some(artifact) = manifest.artifacts.first() else {
            return Self {
                status: ManualSmokeStatus::Unavailable,
                message: "microphone capture completed without an audio artifact".to_string(),
            };
        };
        Self {
            status: ManualSmokeStatus::Passed,
            message: format!(
                "wrote {}: sample_rate_hz={}, channels={}, device={}, duration_ms={}, sha256={}",
                artifact.path.display(),
                artifact.sample_rate_hz,
                artifact.channel_count,
                artifact.identity.display_name,
                artifact.duration_ms,
                artifact.sha256
            ),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioArtifactMetadata {
    pub stream: StreamKind,
    pub file_name: String,
    pub path: PathBuf,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub duration_ms: u64,
    pub sample_rate_hz: u32,
    pub channel_count: u16,
    pub identity: DeviceIdentity,
    pub bytes_written: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactManifest {
    pub recording: RecordingMetadata,
    pub status: ManifestStatus,
    pub ended_at_ms: Option<u64>,
    pub artifacts: Vec<AudioArtifactMetadata>,
    pub recovery: Option<RecoveryMetadata>,
}

#[derive(Debug)]
pub enum RecordingError {
    Io(io::Error),
    Wav(hound::Error),
    StreamNotRequested(StreamKind),
    MissingStreamMetadata(StreamKind),
    MismatchedFrameFormat {
        stream: StreamKind,
        expected_sample_rate_hz: u32,
        actual_sample_rate_hz: u32,
        expected_channel_count: u16,
        actual_channel_count: u16,
    },
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordingError::Io(error) => write!(f, "audio artifact I/O failed: {error}"),
            RecordingError::Wav(error) => write!(f, "WAV artifact write failed: {error}"),
            RecordingError::StreamNotRequested(stream) => {
                write!(
                    f,
                    "frame stream was not requested: {}",
                    stream.as_manifest_str()
                )
            }
            RecordingError::MissingStreamMetadata(stream) => write!(
                f,
                "missing device metadata for requested stream: {}",
                stream.as_manifest_str()
            ),
            RecordingError::MismatchedFrameFormat {
                stream,
                expected_sample_rate_hz,
                actual_sample_rate_hz,
                expected_channel_count,
                actual_channel_count,
            } => write!(
                f,
                "mismatched {} frame format: expected {} Hz/{} channels, got {} Hz/{} channels",
                stream.as_manifest_str(),
                expected_sample_rate_hz,
                expected_channel_count,
                actual_sample_rate_hz,
                actual_channel_count
            ),
        }
    }
}

impl Error for RecordingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RecordingError::Io(error) => Some(error),
            RecordingError::Wav(error) => Some(error),
            RecordingError::StreamNotRequested(_)
            | RecordingError::MissingStreamMetadata(_)
            | RecordingError::MismatchedFrameFormat { .. } => None,
        }
    }
}

impl From<io::Error> for RecordingError {
    fn from(error: io::Error) -> Self {
        RecordingError::Io(error)
    }
}

impl From<hound::Error> for RecordingError {
    fn from(error: hound::Error) -> Self {
        RecordingError::Wav(error)
    }
}

struct ActiveWavArtifact {
    file_name: String,
    path: PathBuf,
    writer: hound::WavWriter<BufWriter<File>>,
    started_at_ms: u64,
    ended_at_ms: Option<u64>,
    sample_rate_hz: u32,
    channel_count: u16,
    identity: DeviceIdentity,
    audio_frame_count: u64,
}

pub struct StreamingWavRecorder {
    session_dir: PathBuf,
    config: CaptureConfiguration,
    manifest: ArtifactManifest,
    snapshot: DeviceSnapshot,
    active: BTreeMap<StreamKind, ActiveWavArtifact>,
}

impl StreamingWavRecorder {
    pub fn start(
        root: &Path,
        recording: RecordingMetadata,
        config: CaptureConfiguration,
        snapshot: DeviceSnapshot,
    ) -> Result<Self, RecordingError> {
        for stream in config.requested_streams() {
            if metadata_for_stream(&snapshot, stream).is_none() {
                return Err(RecordingError::MissingStreamMetadata(stream));
            }
        }

        let session_dir = root.join(&recording.session_id);
        fs::create_dir_all(&session_dir)?;
        let recorder = Self {
            session_dir,
            config,
            manifest: ArtifactManifest {
                recording,
                status: ManifestStatus::Recording,
                ended_at_ms: None,
                artifacts: Vec::new(),
                recovery: None,
            },
            snapshot,
            active: BTreeMap::new(),
        };
        recorder.write_manifest()?;
        Ok(recorder)
    }

    pub fn write_frame(&mut self, frame: &AudioFrame) -> Result<(), RecordingError> {
        if !self.config.requests(frame.stream) {
            return Err(RecordingError::StreamNotRequested(frame.stream));
        }
        if !self.active.contains_key(&frame.stream) {
            let metadata = metadata_for_stream(&self.snapshot, frame.stream)
                .ok_or(RecordingError::MissingStreamMetadata(frame.stream))?;
            let file_name = wav_file_name(frame.stream).to_string();
            let path = self.session_dir.join(&file_name);
            let writer = hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    channels: frame.channel_count,
                    sample_rate: frame.sample_rate_hz,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )?;
            self.active.insert(
                frame.stream,
                ActiveWavArtifact {
                    file_name,
                    path,
                    writer,
                    started_at_ms: frame.start_time_ms,
                    ended_at_ms: None,
                    sample_rate_hz: frame.sample_rate_hz,
                    channel_count: frame.channel_count,
                    identity: metadata.identity,
                    audio_frame_count: 0,
                },
            );
        }

        {
            let artifact = self
                .active
                .get_mut(&frame.stream)
                .ok_or(RecordingError::MissingStreamMetadata(frame.stream))?;
            if artifact.sample_rate_hz != frame.sample_rate_hz
                || artifact.channel_count != frame.channel_count
            {
                return Err(RecordingError::MismatchedFrameFormat {
                    stream: frame.stream,
                    expected_sample_rate_hz: artifact.sample_rate_hz,
                    actual_sample_rate_hz: frame.sample_rate_hz,
                    expected_channel_count: artifact.channel_count,
                    actual_channel_count: frame.channel_count,
                });
            }
            for sample in &frame.pcm_i16 {
                artifact.writer.write_sample(*sample)?;
            }
            artifact.audio_frame_count +=
                frame.pcm_i16.len() as u64 / u64::from(frame.channel_count.max(1));
            artifact.ended_at_ms = Some(frame_end_time_ms(frame));
            artifact.writer.flush()?;
        }
        self.write_manifest()
    }

    pub fn stop(mut self, ended_at_ms: u64) -> Result<ArtifactManifest, RecordingError> {
        self.manifest.status = ManifestStatus::Complete;
        self.manifest.ended_at_ms = Some(ended_at_ms);
        for (stream, artifact) in std::mem::take(&mut self.active) {
            artifact.writer.finalize()?;
            let bytes_written = fs::metadata(&artifact.path)?.len();
            let sha256 = sha256_file(&artifact.path)?;
            self.manifest.artifacts.push(AudioArtifactMetadata {
                stream,
                file_name: artifact.file_name,
                path: artifact.path,
                started_at_ms: artifact.started_at_ms,
                ended_at_ms: artifact.ended_at_ms,
                duration_ms: samples_to_ms_ceil(
                    artifact.audio_frame_count as usize,
                    artifact.sample_rate_hz,
                ),
                sample_rate_hz: artifact.sample_rate_hz,
                channel_count: artifact.channel_count,
                identity: artifact.identity,
                bytes_written,
                sha256,
            });
        }
        self.write_manifest()?;
        Ok(self.manifest)
    }

    fn write_manifest(&self) -> Result<(), RecordingError> {
        let path = self.session_dir.join("manifest.txt");
        let mut file = File::create(path)?;
        writeln!(file, "session_id={}", self.manifest.recording.session_id)?;
        writeln!(file, "status={}", self.manifest.status.as_manifest_str())?;
        writeln!(
            file,
            "started_at_ms={}",
            self.manifest.recording.started_at_ms
        )?;
        writeln!(file, "ended_at_ms={:?}", self.manifest.ended_at_ms)?;
        if let Some(recovery) = &self.manifest.recovery {
            writeln!(file, "recoverable={}", recovery.recoverable)?;
            writeln!(file, "recovery_reason={}", recovery.reason)?;
        } else if self.manifest.status == ManifestStatus::Recording && !self.active.is_empty() {
            writeln!(file, "recoverable=true")?;
            writeln!(
                file,
                "recovery_reason=recording active; WAV artifact can be recovered if interrupted"
            )?;
        }
        for artifact in &self.manifest.artifacts {
            writeln!(
                file,
                "artifact={},{},{},{}",
                artifact.stream.as_manifest_str(),
                artifact.file_name,
                artifact_status_for_manifest(self.manifest.status),
                artifact.bytes_written
            )?;
            writeln!(file, "artifact_started_at_ms={}", artifact.started_at_ms)?;
            writeln!(file, "artifact_ended_at_ms={:?}", artifact.ended_at_ms)?;
            writeln!(file, "duration_ms={}", artifact.duration_ms)?;
            writeln!(file, "sample_rate_hz={}", artifact.sample_rate_hz)?;
            writeln!(file, "channel_count={}", artifact.channel_count)?;
            writeln!(file, "device_identity={}", artifact.identity.identity)?;
            writeln!(
                file,
                "device_display_name={}",
                artifact.identity.display_name
            )?;
            writeln!(file, "device_transport={}", artifact.identity.transport)?;
            writeln!(file, "sha256={}", artifact.sha256)?;
        }
        for (stream, artifact) in &self.active {
            let bytes_written = fs::metadata(&artifact.path)?.len();
            writeln!(
                file,
                "artifact={},{},{},{}",
                stream.as_manifest_str(),
                artifact.file_name,
                "Writing",
                bytes_written
            )?;
            writeln!(file, "artifact_started_at_ms={}", artifact.started_at_ms)?;
            writeln!(file, "artifact_ended_at_ms={:?}", artifact.ended_at_ms)?;
            writeln!(
                file,
                "duration_ms={}",
                samples_to_ms_ceil(artifact.audio_frame_count as usize, artifact.sample_rate_hz)
            )?;
            writeln!(file, "sample_rate_hz={}", artifact.sample_rate_hz)?;
            writeln!(file, "channel_count={}", artifact.channel_count)?;
            writeln!(file, "device_identity={}", artifact.identity.identity)?;
            writeln!(
                file,
                "device_display_name={}",
                artifact.identity.display_name
            )?;
            writeln!(file, "device_transport={}", artifact.identity.transport)?;
        }
        file.sync_data()?;
        Ok(())
    }
}

fn metadata_for_stream(snapshot: &DeviceSnapshot, stream: StreamKind) -> Option<StreamMetadata> {
    match stream {
        StreamKind::Microphone => snapshot.mic.clone(),
        StreamKind::SystemAudio => snapshot.system.clone(),
    }
}

fn wav_file_name(stream: StreamKind) -> &'static str {
    match stream {
        StreamKind::Microphone => "raw-mic.wav",
        StreamKind::SystemAudio => "raw-system.wav",
    }
}

fn artifact_status_for_manifest(status: ManifestStatus) -> &'static str {
    match status {
        ManifestStatus::Recording => "Recording",
        ManifestStatus::Complete => "Complete",
        ManifestStatus::Canceled => "Interrupted",
        ManifestStatus::Failed => "Interrupted",
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemAudioAdapterStatus {
    Available,
    PermissionDenied(CapturePermissionError),
    Unavailable(CaptureUnavailable),
}

pub struct ScreenCaptureKitSystemAudioAdapter;

impl ScreenCaptureKitSystemAudioAdapter {
    pub fn status() -> SystemAudioAdapterStatus {
        #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
        {
            probe_screen_capturekit_system_audio()
        }
        #[cfg(all(target_os = "macos", not(feature = "system-audio-screencapturekit")))]
        {
            SystemAudioAdapterStatus::Unavailable(CaptureUnavailable::system_audio(
                "ScreenCaptureKit system audio capture is available behind the system-audio-screencapturekit feature",
            ))
        }
        #[cfg(not(target_os = "macos"))]
        {
            SystemAudioAdapterStatus::Unavailable(CaptureUnavailable::system_audio(
                "ScreenCaptureKit system audio capture requires macOS",
            ))
        }
    }

    pub fn start_capture(&self) -> Result<(), CaptureError> {
        match Self::status() {
            SystemAudioAdapterStatus::Available => Ok(()),
            SystemAudioAdapterStatus::PermissionDenied(error) => {
                Err(CaptureError::PermissionDenied(error))
            }
            SystemAudioAdapterStatus::Unavailable(error) => Err(CaptureError::Unavailable(error)),
        }
    }
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SystemAudioSampleEncoding {
    Float32Le,
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SystemAudioRawBuffer<'a> {
    channel_count: u16,
    data: &'a [u8],
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
enum SystemAudioWriterMessage {
    Samples(Vec<i16>),
    #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
    Stop {
        ended_at_ms: u64,
    },
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
fn system_audio_buffers_to_i16(
    buffers: &[SystemAudioRawBuffer<'_>],
    encoding: SystemAudioSampleEncoding,
) -> Result<Vec<i16>, CaptureError> {
    if buffers.is_empty() {
        return Ok(Vec::new());
    }
    if buffers.iter().any(|buffer| buffer.channel_count == 0) {
        return Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
            "system audio buffer reported zero channels",
        )));
    }

    if buffers.len() > 1 && buffers.iter().all(|buffer| buffer.channel_count == 1) {
        let planes = buffers
            .iter()
            .map(|buffer| decode_system_audio_buffer(buffer.data, encoding))
            .collect::<Result<Vec<_>, _>>()?;
        let frame_count = planes.first().map(Vec::len).unwrap_or(0);
        if planes.iter().any(|plane| plane.len() != frame_count) {
            return Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
                "system audio planar buffers have mismatched frame counts",
            )));
        }
        let mut samples = Vec::with_capacity(frame_count * planes.len());
        for frame_index in 0..frame_count {
            for plane in &planes {
                samples.push(plane[frame_index]);
            }
        }
        return Ok(samples);
    }

    let mut samples = Vec::new();
    for buffer in buffers {
        samples.extend(decode_system_audio_buffer(buffer.data, encoding)?);
    }
    Ok(samples)
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
fn decode_system_audio_buffer(
    data: &[u8],
    encoding: SystemAudioSampleEncoding,
) -> Result<Vec<i16>, CaptureError> {
    match encoding {
        SystemAudioSampleEncoding::Float32Le => {
            if data.len() % std::mem::size_of::<f32>() != 0 {
                return Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
                    "system audio float buffer was not aligned to 32-bit samples",
                )));
            }
            Ok(data
                .chunks_exact(std::mem::size_of::<f32>())
                .map(|bytes| {
                    pcm_f32_to_i16(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                })
                .collect())
        }
    }
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
fn send_system_audio_samples(
    tx: &std::sync::mpsc::SyncSender<SystemAudioWriterMessage>,
    errors: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    samples: Vec<i16>,
) {
    if let Err(error) = tx.try_send(SystemAudioWriterMessage::Samples(samples)) {
        if let Ok(mut errors) = errors.lock() {
            errors.push(format!("system audio writer backpressure: {error}"));
        }
    }
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
fn system_audio_capture_stream_result(
    wrote_samples: bool,
    stream_errors: &[String],
) -> Result<(), CaptureError> {
    if let Some(stream_error) = stream_errors.first() {
        return Err(system_audio_error_from_message(stream_error.clone()));
    }
    if !wrote_samples {
        return Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
            "system audio stream produced no samples",
        )));
    }
    Ok(())
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
fn system_audio_error_from_message(message: String) -> CaptureError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("declined")
        || lower.contains("not authorized")
        || lower.contains("not authorised")
        || lower.contains("authorization")
    {
        CaptureError::PermissionDenied(CapturePermissionError::denied(
            CapturePermission::SystemAudioScreenRecording,
        ))
    } else {
        CaptureError::Unavailable(CaptureUnavailable::system_audio(message))
    }
}

#[cfg(any(
    test,
    all(target_os = "macos", feature = "system-audio-screencapturekit")
))]
fn pcm_f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

#[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
pub struct MacosSystemAudioWavRecording {
    stream: screencapturekit::prelude::SCStream,
    sample_tx: Option<std::sync::mpsc::SyncSender<SystemAudioWriterMessage>>,
    writer: Option<std::thread::JoinHandle<Result<ArtifactManifest, CaptureError>>>,
    stream_errors: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    sample_rate_hz: u32,
    channel_count: u16,
}

#[cfg(not(all(target_os = "macos", feature = "system-audio-screencapturekit")))]
pub struct MacosSystemAudioWavRecording;

impl MacosSystemAudioWavRecording {
    #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
    pub fn start(root: &Path, session_id: &str, started_at_ms: u64) -> Result<Self, CaptureError> {
        use screencapturekit::prelude::*;
        use std::sync::{mpsc, Arc, Mutex};

        let content = SCShareableContent::get()
            .map_err(|error| system_audio_error_from_message(error.to_string()))?;
        let display = content.displays().into_iter().next().ok_or_else(|| {
            CaptureError::Unavailable(CaptureUnavailable::system_audio(
                "ScreenCaptureKit did not report any capturable displays",
            ))
        })?;
        let display_id = display.display_id();
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();
        let sample_rate_hz = 48_000;
        let channel_count = 2;
        let config = SCStreamConfiguration::new()
            .with_width(display.width())
            .with_height(display.height())
            .with_captures_audio(true)
            .with_sample_rate(sample_rate_hz as i32)
            .with_channel_count(i32::from(channel_count));
        let (sample_tx, sample_rx) = mpsc::sync_channel::<SystemAudioWriterMessage>(32);
        let stream_errors = Arc::new(Mutex::new(Vec::<String>::new()));
        let snapshot = DeviceSnapshot {
            captured_at_ms: started_at_ms,
            mic: None,
            system: Some(StreamMetadata {
                stream: StreamKind::SystemAudio,
                sample_rate_hz,
                channel_count,
                identity: DeviceIdentity::new(
                    &format!("screencapturekit-display-{display_id}"),
                    "macOS system audio",
                    "ScreenCaptureKit",
                ),
                start_time_ms: started_at_ms,
            }),
        };
        let recorder = StreamingWavRecorder::start(
            root,
            RecordingMetadata::new(session_id, started_at_ms),
            CaptureConfiguration::system_only()?,
            snapshot,
        )?;
        let writer_errors = Arc::clone(&stream_errors);
        let writer = std::thread::spawn(move || {
            run_system_audio_writer(
                recorder,
                sample_rx,
                writer_errors,
                started_at_ms,
                sample_rate_hz,
                channel_count,
            )
        });

        let handler_tx = sample_tx.clone();
        let handler_errors = Arc::clone(&stream_errors);
        let mut stream = SCStream::new(&filter, &config);
        if stream
            .add_output_handler(
                move |sample: CMSampleBuffer, of_type: SCStreamOutputType| {
                    if of_type != SCStreamOutputType::Audio {
                        return;
                    }
                    let Some(audio_buffers) = sample.audio_buffer_list() else {
                        if let Ok(mut errors) = handler_errors.lock() {
                            errors.push(
                                "system audio sample did not include audio buffers".to_string(),
                            );
                        }
                        return;
                    };
                    let raw_buffers = audio_buffers
                        .iter()
                        .map(|buffer| SystemAudioRawBuffer {
                            channel_count: buffer.number_channels as u16,
                            data: buffer.data(),
                        })
                        .collect::<Vec<_>>();
                    match system_audio_buffers_to_i16(
                        &raw_buffers,
                        SystemAudioSampleEncoding::Float32Le,
                    ) {
                        Ok(samples) => {
                            if !samples.is_empty() {
                                send_system_audio_samples(&handler_tx, &handler_errors, samples);
                            }
                        }
                        Err(error) => {
                            if let Ok(mut errors) = handler_errors.lock() {
                                errors.push(error.to_string());
                            }
                        }
                    }
                },
                SCStreamOutputType::Audio,
            )
            .is_none()
        {
            let _ = sample_tx.send(SystemAudioWriterMessage::Stop {
                ended_at_ms: started_at_ms,
            });
            drop(sample_tx);
            let _ = writer.join();
            return Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
                "ScreenCaptureKit did not accept a system audio output handler",
            )));
        }

        if let Err(error) = stream.start_capture() {
            let _ = sample_tx.send(SystemAudioWriterMessage::Stop {
                ended_at_ms: started_at_ms,
            });
            drop(sample_tx);
            let _ = writer.join();
            return Err(system_audio_error_from_message(error.to_string()));
        }

        Ok(Self {
            stream,
            sample_tx: Some(sample_tx),
            writer: Some(writer),
            stream_errors,
            sample_rate_hz,
            channel_count,
        })
    }

    #[cfg(not(all(target_os = "macos", feature = "system-audio-screencapturekit")))]
    pub fn start(
        _root: &Path,
        _session_id: &str,
        _started_at_ms: u64,
    ) -> Result<Self, CaptureError> {
        Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
            "ScreenCaptureKit system audio capture requires macOS and the system-audio-screencapturekit feature",
        )))
    }

    #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    #[cfg(not(all(target_os = "macos", feature = "system-audio-screencapturekit")))]
    pub fn sample_rate_hz(&self) -> u32 {
        0
    }

    #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    #[cfg(not(all(target_os = "macos", feature = "system-audio-screencapturekit")))]
    pub fn channel_count(&self) -> u16 {
        0
    }

    #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
    pub fn stop(mut self, ended_at_ms: u64) -> Result<ArtifactManifest, CaptureError> {
        if let Err(error) = self.stream.stop_capture() {
            if let Ok(mut errors) = self.stream_errors.lock() {
                errors.push(error.to_string());
            }
        }
        let sample_tx = self.sample_tx.take().ok_or_else(|| {
            CaptureError::Unavailable(CaptureUnavailable::system_audio(
                "system audio writer channel is unavailable",
            ))
        })?;
        sample_tx
            .send(SystemAudioWriterMessage::Stop { ended_at_ms })
            .map_err(|_| {
                CaptureError::Unavailable(CaptureUnavailable::system_audio(
                    "system audio writer stopped before finalizing the WAV artifact",
                ))
            })?;
        drop(sample_tx);
        let writer = self.writer.take().ok_or_else(|| {
            CaptureError::Unavailable(CaptureUnavailable::system_audio(
                "system audio writer task is unavailable",
            ))
        })?;
        writer.join().map_err(|_| {
            CaptureError::Unavailable(CaptureUnavailable::system_audio(
                "system audio writer task panicked while finalizing the WAV artifact",
            ))
        })?
    }

    #[cfg(not(all(target_os = "macos", feature = "system-audio-screencapturekit")))]
    pub fn stop(self, _ended_at_ms: u64) -> Result<ArtifactManifest, CaptureError> {
        Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
            "ScreenCaptureKit system audio capture requires macOS and the system-audio-screencapturekit feature",
        )))
    }
}

#[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
fn probe_screen_capturekit_system_audio() -> SystemAudioAdapterStatus {
    use screencapturekit::prelude::*;

    match SCShareableContent::get() {
        Ok(content) if !content.displays().is_empty() => SystemAudioAdapterStatus::Available,
        Ok(_) => SystemAudioAdapterStatus::Unavailable(CaptureUnavailable::system_audio(
            "ScreenCaptureKit did not report any capturable displays",
        )),
        Err(error) => match system_audio_error_from_message(error.to_string()) {
            CaptureError::PermissionDenied(error) => {
                SystemAudioAdapterStatus::PermissionDenied(error)
            }
            CaptureError::Unavailable(error) => SystemAudioAdapterStatus::Unavailable(error),
            CaptureError::Configuration(_) | CaptureError::Recording(_) => {
                SystemAudioAdapterStatus::Unavailable(CaptureUnavailable::system_audio(
                    "ScreenCaptureKit system audio status probe failed",
                ))
            }
        },
    }
}

#[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
fn run_system_audio_writer(
    mut recorder: StreamingWavRecorder,
    sample_rx: std::sync::mpsc::Receiver<SystemAudioWriterMessage>,
    stream_errors: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    started_at_ms: u64,
    sample_rate_hz: u32,
    channel_count: u16,
) -> Result<ArtifactManifest, CaptureError> {
    let mut wrote_samples = false;
    let mut frame_start_ms = started_at_ms;
    for message in sample_rx {
        match message {
            SystemAudioWriterMessage::Samples(pcm_i16) => {
                if pcm_i16.is_empty() {
                    continue;
                }
                wrote_samples = true;
                let frame = AudioFrame {
                    stream: StreamKind::SystemAudio,
                    start_time_ms: frame_start_ms,
                    sample_rate_hz,
                    channel_count,
                    pcm_i16,
                };
                frame_start_ms = frame_end_time_ms(&frame);
                recorder.write_frame(&frame)?;
            }
            SystemAudioWriterMessage::Stop { ended_at_ms } => {
                let stream_errors = stream_errors
                    .lock()
                    .map(|errors| errors.clone())
                    .unwrap_or_default();
                system_audio_capture_stream_result(wrote_samples, &stream_errors)?;
                return recorder.stop(ended_at_ms).map_err(CaptureError::from);
            }
        }
    }
    Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
        "system audio writer stopped before finalizing the WAV artifact",
    )))
}

#[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
pub fn record_macos_system_audio_to_wav(
    root: &Path,
    duration: Duration,
) -> Result<ArtifactManifest, CaptureError> {
    let started_at_ms = now_ms();
    let recording = MacosSystemAudioWavRecording::start(root, "system-audio-smoke", started_at_ms)?;
    std::thread::sleep(duration);
    recording.stop(now_ms())
}

#[cfg(not(all(target_os = "macos", feature = "system-audio-screencapturekit")))]
pub fn record_macos_system_audio_to_wav(
    _root: &Path,
    _duration: Duration,
) -> Result<ArtifactManifest, CaptureError> {
    Err(CaptureError::Unavailable(CaptureUnavailable::system_audio(
        "ScreenCaptureKit system audio capture requires macOS and the system-audio-screencapturekit feature",
    )))
}

#[cfg(target_os = "macos")]
pub struct MacosMicrophoneWavRecording {
    stream: cpal::Stream,
    sample_tx: Option<std::sync::mpsc::SyncSender<MicrophoneWriterMessage>>,
    writer: Option<std::thread::JoinHandle<Result<ArtifactManifest, CaptureError>>>,
    sample_rate_hz: u32,
}

#[cfg(target_os = "macos")]
enum MicrophoneWriterMessage {
    Samples(Vec<i16>),
    Stop { ended_at_ms: u64 },
}

#[cfg(not(target_os = "macos"))]
pub struct MacosMicrophoneWavRecording;

impl MacosMicrophoneWavRecording {
    #[cfg(target_os = "macos")]
    pub fn start(root: &Path, session_id: &str, started_at_ms: u64) -> Result<Self, CaptureError> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use std::sync::{mpsc, Arc, Mutex};

        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| {
            CaptureError::Unavailable(CaptureUnavailable::microphone(
                "no default macOS input device is available",
            ))
        })?;
        let device_name = device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|_| "Default input device".to_string());
        let supported_config = device
            .default_input_config()
            .map_err(|error| microphone_error_from_message(error.to_string()))?;
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.clone().into();
        let sample_rate_hz = stream_config.sample_rate;
        let channel_count = stream_config.channels;
        let (sample_tx, sample_rx) = mpsc::sync_channel::<MicrophoneWriterMessage>(32);
        let stream_errors = Arc::new(Mutex::new(Vec::<String>::new()));

        let stream = build_macos_input_stream(
            &device,
            &stream_config,
            sample_format,
            sample_tx.clone(),
            Arc::clone(&stream_errors),
        )?;
        let snapshot = DeviceSnapshot {
            captured_at_ms: started_at_ms,
            mic: Some(StreamMetadata {
                stream: StreamKind::Microphone,
                sample_rate_hz,
                channel_count,
                identity: DeviceIdentity::new("macos-default-input", &device_name, "cpal"),
                start_time_ms: started_at_ms,
            }),
            system: None,
        };
        let recorder = StreamingWavRecorder::start(
            root,
            RecordingMetadata::new(session_id, started_at_ms),
            CaptureConfiguration::mic_only()?,
            snapshot,
        )?;
        let writer_errors = Arc::clone(&stream_errors);
        let writer = std::thread::spawn(move || {
            run_microphone_writer(
                recorder,
                sample_rx,
                writer_errors,
                started_at_ms,
                sample_rate_hz,
                channel_count,
            )
        });
        stream
            .play()
            .map_err(|error| microphone_error_from_message(error.to_string()))?;

        Ok(Self {
            stream,
            sample_tx: Some(sample_tx),
            writer: Some(writer),
            sample_rate_hz,
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn start(
        _root: &Path,
        _session_id: &str,
        _started_at_ms: u64,
    ) -> Result<Self, CaptureError> {
        Err(CaptureError::Unavailable(CaptureUnavailable::microphone(
            "macOS microphone capture requires macOS",
        )))
    }

    #[cfg(target_os = "macos")]
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    #[cfg(not(target_os = "macos"))]
    pub fn sample_rate_hz(&self) -> u32 {
        0
    }

    #[cfg(target_os = "macos")]
    pub fn stop(mut self, ended_at_ms: u64) -> Result<ArtifactManifest, CaptureError> {
        drop(self.stream);
        let sample_tx = self.sample_tx.take().ok_or_else(|| {
            CaptureError::Unavailable(CaptureUnavailable::microphone(
                "microphone writer channel is unavailable",
            ))
        })?;
        sample_tx
            .send(MicrophoneWriterMessage::Stop { ended_at_ms })
            .map_err(|_| {
                CaptureError::Unavailable(CaptureUnavailable::microphone(
                    "microphone writer stopped before finalizing the WAV artifact",
                ))
            })?;
        drop(sample_tx);
        let writer = self.writer.take().ok_or_else(|| {
            CaptureError::Unavailable(CaptureUnavailable::microphone(
                "microphone writer task is unavailable",
            ))
        })?;
        writer.join().map_err(|_| {
            CaptureError::Unavailable(CaptureUnavailable::microphone(
                "microphone writer task panicked while finalizing the WAV artifact",
            ))
        })?
    }

    #[cfg(not(target_os = "macos"))]
    pub fn stop(self, _ended_at_ms: u64) -> Result<ArtifactManifest, CaptureError> {
        Err(CaptureError::Unavailable(CaptureUnavailable::microphone(
            "macOS microphone capture requires macOS",
        )))
    }
}

#[cfg(target_os = "macos")]
fn run_microphone_writer(
    mut recorder: StreamingWavRecorder,
    sample_rx: std::sync::mpsc::Receiver<MicrophoneWriterMessage>,
    stream_errors: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    started_at_ms: u64,
    sample_rate_hz: u32,
    channel_count: u16,
) -> Result<ArtifactManifest, CaptureError> {
    let mut wrote_samples = false;
    let mut frame_start_ms = started_at_ms;
    for message in sample_rx {
        match message {
            MicrophoneWriterMessage::Samples(pcm_i16) => {
                if pcm_i16.is_empty() {
                    continue;
                }
                wrote_samples = true;
                let frame = AudioFrame {
                    stream: StreamKind::Microphone,
                    start_time_ms: frame_start_ms,
                    sample_rate_hz,
                    channel_count,
                    pcm_i16,
                };
                frame_start_ms = frame_end_time_ms(&frame);
                recorder.write_frame(&frame)?;
            }
            MicrophoneWriterMessage::Stop { ended_at_ms } => {
                let stream_errors = stream_errors
                    .lock()
                    .map(|errors| errors.clone())
                    .unwrap_or_default();
                microphone_capture_stream_result(wrote_samples, &stream_errors)?;
                return recorder.stop(ended_at_ms).map_err(CaptureError::from);
            }
        }
    }
    Err(CaptureError::Unavailable(CaptureUnavailable::microphone(
        "microphone writer stopped before finalizing the WAV artifact",
    )))
}

#[cfg(target_os = "macos")]
pub fn record_macos_microphone_to_wav(
    root: &Path,
    duration: Duration,
) -> Result<ArtifactManifest, CaptureError> {
    let started_at_ms = now_ms();
    let recording = MacosMicrophoneWavRecording::start(root, "mic-smoke", started_at_ms)?;
    std::thread::sleep(duration);
    recording.stop(now_ms())
}

#[cfg(not(target_os = "macos"))]
pub fn record_macos_microphone_to_wav(
    _root: &Path,
    _duration: Duration,
) -> Result<ArtifactManifest, CaptureError> {
    Err(CaptureError::Unavailable(CaptureUnavailable::microphone(
        "macOS microphone capture requires macOS",
    )))
}

#[cfg(target_os = "macos")]
fn build_macos_input_stream(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    sample_tx: std::sync::mpsc::SyncSender<MicrophoneWriterMessage>,
    stream_errors: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> Result<cpal::Stream, CaptureError> {
    use cpal::traits::DeviceTrait;

    match sample_format {
        cpal::SampleFormat::I8 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[i8], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter().map(|sample| i16::from(*sample) << 8).collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[i16], _| {
                    send_mic_samples(&tx, &errors, data.to_vec());
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::I32 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[i32], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter().map(|sample| (sample >> 16) as i16).collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::I64 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[i64], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter().map(|sample| (sample >> 48) as i16).collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::U8 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[u8], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter()
                            .map(|sample| (i16::from(*sample) - 128) << 8)
                            .collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[u16], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter()
                            .map(|sample| (*sample as i32 - 32_768) as i16)
                            .collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::U32 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[u32], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter()
                            .map(|sample| ((*sample as i64 - 2_147_483_648) >> 16) as i16)
                            .collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::U64 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[u64], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter()
                            .map(|sample| {
                                ((*sample as i128 - 9_223_372_036_854_775_808i128) >> 48) as i16
                            })
                            .collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::F32 => {
            let tx = sample_tx.clone();
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[f32], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter().map(|sample| f32_to_i16(*sample)).collect(),
                    );
                },
                cpal_error_handler(std::sync::Arc::clone(&stream_errors)),
                None,
            )
        }
        cpal::SampleFormat::F64 => {
            let tx = sample_tx;
            let errors = std::sync::Arc::clone(&stream_errors);
            device.build_input_stream(
                stream_config,
                move |data: &[f64], _| {
                    send_mic_samples(
                        &tx,
                        &errors,
                        data.iter()
                            .map(|sample| f32_to_i16(*sample as f32))
                            .collect(),
                    );
                },
                cpal_error_handler(stream_errors),
                None,
            )
        }
        unsupported => {
            return Err(CaptureError::Unavailable(CaptureUnavailable::microphone(
                format!("unsupported default input sample format: {unsupported:?}"),
            )));
        }
    }
    .map_err(|error| microphone_error_from_message(error.to_string()))
}

#[cfg(target_os = "macos")]
fn send_mic_samples(
    tx: &std::sync::mpsc::SyncSender<MicrophoneWriterMessage>,
    errors: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    samples: Vec<i16>,
) {
    if let Err(error) = tx.try_send(MicrophoneWriterMessage::Samples(samples)) {
        if let Ok(mut errors) = errors.lock() {
            errors.push(format!("microphone writer backpressure: {error}"));
        }
    }
}

#[cfg(target_os = "macos")]
fn cpal_error_handler(
    errors: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> impl FnMut(cpal::StreamError) + Send + 'static {
    move |error| {
        if let Ok(mut errors) = errors.lock() {
            errors.push(error.to_string());
        }
    }
}

#[cfg(target_os = "macos")]
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

fn microphone_error_from_message(message: String) -> CaptureError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("permission") || lower.contains("denied") || lower.contains("authorized") {
        CaptureError::PermissionDenied(CapturePermissionError::denied(
            CapturePermission::Microphone,
        ))
    } else {
        CaptureError::Unavailable(CaptureUnavailable::microphone(message))
    }
}

fn microphone_capture_stream_result(
    wrote_samples: bool,
    stream_errors: &[String],
) -> Result<(), CaptureError> {
    if let Some(stream_error) = stream_errors.first() {
        return Err(microphone_error_from_message(stream_error.clone()));
    }
    if !wrote_samples {
        return Err(CaptureError::Unavailable(CaptureUnavailable::microphone(
            "microphone stream produced no samples",
        )));
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_stream_errors_fail_even_after_samples_arrive() {
        let result = microphone_capture_stream_result(true, &["DeviceNotAvailable".to_string()]);

        assert!(matches!(result, Err(CaptureError::Unavailable(_))));
    }

    #[test]
    fn system_audio_interleaved_f32_bytes_convert_to_pcm_i16() {
        let bytes = [
            0.0f32.to_le_bytes(),
            1.0f32.to_le_bytes(),
            (-1.0f32).to_le_bytes(),
            0.5f32.to_le_bytes(),
        ]
        .concat();
        let buffers = [SystemAudioRawBuffer {
            channel_count: 2,
            data: &bytes,
        }];

        let samples = system_audio_buffers_to_i16(&buffers, SystemAudioSampleEncoding::Float32Le)
            .expect("convert samples");

        assert_eq!(samples, vec![0, 32767, -32767, 16383]);
    }

    #[test]
    fn system_audio_planar_f32_buffers_are_interleaved_for_wav_output() {
        let left = [0.25f32.to_le_bytes(), 0.5f32.to_le_bytes()].concat();
        let right = [(-0.25f32).to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
        let buffers = [
            SystemAudioRawBuffer {
                channel_count: 1,
                data: &left,
            },
            SystemAudioRawBuffer {
                channel_count: 1,
                data: &right,
            },
        ];

        let samples = system_audio_buffers_to_i16(&buffers, SystemAudioSampleEncoding::Float32Le)
            .expect("convert planar samples");

        assert_eq!(samples, vec![8191, -8191, 16383, -16383]);
    }

    #[test]
    fn system_audio_writer_backpressure_fails_loudly_on_stop() {
        let (tx, _rx) = std::sync::mpsc::sync_channel(0);
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        send_system_audio_samples(&tx, &errors, vec![1, 2]);
        let errors = errors.lock().expect("errors").clone();
        let result = system_audio_capture_stream_result(true, &errors);

        assert!(
            matches!(result, Err(CaptureError::Unavailable(error)) if error.capability == CaptureCapability::SystemAudio)
        );
        assert!(errors[0].contains("system audio writer backpressure"));
    }

    #[test]
    fn system_audio_sender_delivers_pcm_samples_when_writer_has_capacity() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        send_system_audio_samples(&tx, &errors, vec![10, -10]);

        match rx.try_recv().expect("sample message") {
            SystemAudioWriterMessage::Samples(samples) => assert_eq!(samples, vec![10, -10]),
            #[cfg(all(target_os = "macos", feature = "system-audio-screencapturekit"))]
            SystemAudioWriterMessage::Stop { .. } => panic!("unexpected stop message"),
        }
        assert!(errors.lock().expect("errors").is_empty());
    }

    #[test]
    fn system_audio_stop_without_samples_is_a_visible_capture_failure() {
        let result = system_audio_capture_stream_result(false, &[]);

        assert!(
            matches!(result, Err(CaptureError::Unavailable(error)) if error.reason.contains("produced no samples"))
        );
    }

    #[test]
    fn system_audio_permission_errors_keep_screen_recording_guidance() {
        let result = system_audio_error_from_message(
            "ScreenCaptureKit user declined screen recording permission".to_string(),
        );

        assert!(matches!(
            result,
            CaptureError::PermissionDenied(CapturePermissionError {
                permission: CapturePermission::SystemAudioScreenRecording,
                ..
            })
        ));
    }
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
        writeln!(
            file,
            "started_at_ms={}",
            self.manifest.recording.started_at_ms
        )?;
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
    let duration_ms = audio_frames
        .saturating_mul(1_000)
        .div_ceil(frame.sample_rate_hz as u64);
    frame.start_time_ms.saturating_add(duration_ms)
}

fn samples_to_ms(samples: usize, sample_rate_hz: u32) -> u64 {
    ((samples as u64) * 1_000) / sample_rate_hz as u64
}

fn samples_to_ms_ceil(samples: usize, sample_rate_hz: u32) -> u64 {
    if samples == 0 || sample_rate_hz == 0 {
        return 0;
    }
    ((samples as u64) * 1_000).div_ceil(sample_rate_hz as u64)
}
