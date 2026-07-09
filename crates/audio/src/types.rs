use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

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

/// Error returned when a capture configuration cannot represent a valid request.
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

/// Boundary for code that can inspect audio devices and produce captured frames.
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

/// Permission failure for a requested capture capability.
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

/// Capability failure when the platform or device set cannot satisfy capture.
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

/// Top-level capture failure exposed by audio setup and recording entry points.
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

/// Error returned when requested audio streams cannot produce a valid artifact.
#[derive(Debug)]
pub enum RecordingError {
    Io(io::Error),
    Wav(hound::Error),
    StreamNotRequested(StreamKind),
    MissingStreamMetadata(StreamKind),
    MissingRequestedStream(StreamKind),
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
            RecordingError::MissingRequestedStream(stream) => write!(
                f,
                "requested stream produced no samples: {}",
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
            | RecordingError::MissingRequestedStream(_)
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
