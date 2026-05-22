use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use curiosity_domain::{SourceChannel, TranscriptSegment};
use hound::{SampleFormat, WavReader};
use sha2::{Digest, Sha256};

#[cfg(feature = "whisper-rs")]
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub type TranscriptionResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const MISSING_MODEL_GUIDANCE: &str = concat!(
    "Set CURIOSITY_WHISPER_MODEL to a local whisper.cpp model file such as ",
    "ggml-base.en.bin, then retry transcription."
);
const UNSUPPORTED_AUDIO_GUIDANCE: &str = concat!(
    "Local Whisper transcription accepts PCM or IEEE-float WAV input. Provide a ",
    "readable WAV artifact from the recorder or convert the source audio to WAV."
);
const AUDIO_UNAVAILABLE_GUIDANCE: &str = concat!(
    "The audio file does not exist or is not readable. Verify the recorder output ",
    "path and file permissions, then retry transcription."
);
const WHISPER_SMOKE_MODEL_AND_WAV_GUIDANCE: &str = concat!(
    "Set CURIOSITY_WHISPER_MODEL and CURIOSITY_WHISPER_WAV to run the real local ",
    "Whisper smoke."
);
const WHISPER_SMOKE_WAV_GUIDANCE: &str = concat!(
    "Set CURIOSITY_WHISPER_WAV with CURIOSITY_WHISPER_MODEL to run the real local ",
    "Whisper smoke."
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelState {
    Missing,
    Downloading {
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    Ready {
        model_name: String,
        sha256: String,
    },
    FailedHash {
        expected_sha256: String,
        actual_sha256: String,
    },
    IncompatibleHardware {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioFixture {
    pub meeting_id: String,
    pub source_artifact_sha256: String,
    pub lines: Vec<FixtureLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub source_channel: SourceChannel,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptionDocument {
    pub provider: String,
    pub model_name: String,
    pub model_run_id: String,
    pub transcript_version_id: String,
    pub source_artifact_sha256: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptionError {
    MissingModelPath { path: PathBuf, guidance: String },
    EmptyAudioInput { guidance: String },
    AudioInputUnavailable { path: PathBuf, guidance: String },
    UnsupportedAudioInput { path: PathBuf, guidance: String },
    BackendUnavailable { provider: String, guidance: String },
    BackendFailed { provider: String, message: String },
}

impl fmt::Display for TranscriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingModelPath { path, guidance } => {
                write!(
                    formatter,
                    "Whisper model path does not exist: {}. {guidance}",
                    path.display()
                )
            }
            Self::EmptyAudioInput { guidance } => {
                write!(formatter, "No audio input was provided. {guidance}")
            }
            Self::AudioInputUnavailable { path, guidance } => {
                write!(
                    formatter,
                    "Audio input is unavailable: {}. {guidance}",
                    path.display()
                )
            }
            Self::UnsupportedAudioInput { path, guidance } => {
                write!(
                    formatter,
                    "Unsupported audio input: {}. {guidance}",
                    path.display()
                )
            }
            Self::BackendUnavailable { provider, guidance } => {
                write!(
                    formatter,
                    "{provider} transcription is unavailable. {guidance}"
                )
            }
            Self::BackendFailed { provider, message } => {
                write!(formatter, "{provider} transcription failed: {message}")
            }
        }
    }
}

impl std::error::Error for TranscriptionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhisperTranscriptionRequest {
    pub meeting_id: String,
    pub audio_path: PathBuf,
    pub source_artifact_sha256: String,
    pub source_channel: SourceChannel,
}

impl WhisperTranscriptionRequest {
    pub fn new(
        meeting_id: impl Into<String>,
        audio_path: impl Into<PathBuf>,
        source_artifact_sha256: impl Into<String>,
        source_channel: SourceChannel,
    ) -> Self {
        Self {
            meeting_id: meeting_id.into(),
            audio_path: audio_path.into(),
            source_artifact_sha256: source_artifact_sha256.into(),
            source_channel,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhisperBackendSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

impl WhisperBackendSegment {
    pub fn new(start_ms: u64, end_ms: u64, text: impl Into<String>) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.into(),
        }
    }
}

pub trait WhisperBackend {
    fn provider(&self) -> &'static str;
    fn transcribe(
        &self,
        model_path: &Path,
        audio_path: &Path,
    ) -> Result<Vec<WhisperBackendSegment>, TranscriptionError>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeWhisperBackend {
    segments: Vec<WhisperBackendSegment>,
}

impl FakeWhisperBackend {
    pub fn new(segments: Vec<WhisperBackendSegment>) -> Self {
        Self { segments }
    }
}

impl WhisperBackend for FakeWhisperBackend {
    fn provider(&self) -> &'static str {
        "local-whisper"
    }

    fn transcribe(
        &self,
        _model_path: &Path,
        _audio_path: &Path,
    ) -> Result<Vec<WhisperBackendSegment>, TranscriptionError> {
        Ok(self.segments.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhisperTranscriber<B> {
    model_path: PathBuf,
    model_name: String,
    backend: B,
}

impl<B> WhisperTranscriber<B>
where
    B: WhisperBackend,
{
    pub fn new(model_path: impl Into<PathBuf>, model_name: impl Into<String>, backend: B) -> Self {
        Self {
            model_path: model_path.into(),
            model_name: model_name.into(),
            backend,
        }
    }

    pub fn transcribe_wav(
        &self,
        request: &WhisperTranscriptionRequest,
    ) -> Result<TranscriptionDocument, TranscriptionError> {
        if !self.model_path.is_file() {
            return Err(TranscriptionError::MissingModelPath {
                path: self.model_path.clone(),
                guidance: MISSING_MODEL_GUIDANCE.to_string(),
            });
        }

        validate_wav_input(&request.audio_path)?;

        let model_run_id = model_run_id(
            self.backend.provider(),
            &self.model_name,
            &request.meeting_id,
            &request.source_artifact_sha256,
        );
        let transcript_version_id = format!("{model_run_id}-v1");
        let mut backend_segments = self
            .backend
            .transcribe(&self.model_path, &request.audio_path)?;
        backend_segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));

        let segments = backend_segments
            .into_iter()
            .enumerate()
            .map(|(index, segment)| {
                TranscriptSegment::with_metadata(
                    format!("{transcript_version_id}-segment-{index}"),
                    request.meeting_id.clone(),
                    segment.start_ms,
                    segment.end_ms,
                    segment.text,
                    request.source_channel,
                    model_run_id.clone(),
                    transcript_version_id.clone(),
                )
            })
            .collect();

        Ok(TranscriptionDocument {
            provider: self.backend.provider().to_string(),
            model_name: self.model_name.clone(),
            model_run_id,
            transcript_version_id,
            source_artifact_sha256: request.source_artifact_sha256.clone(),
            segments,
        })
    }

    pub fn transcribe_wav_bundle(
        &self,
        requests: &[WhisperTranscriptionRequest],
    ) -> Result<TranscriptionDocument, TranscriptionError> {
        if requests.is_empty() {
            return Err(TranscriptionError::EmptyAudioInput {
                guidance: AUDIO_UNAVAILABLE_GUIDANCE.to_string(),
            });
        }
        if requests.len() == 1 {
            return self.transcribe_wav(&requests[0]);
        }
        if !self.model_path.is_file() {
            return Err(TranscriptionError::MissingModelPath {
                path: self.model_path.clone(),
                guidance: MISSING_MODEL_GUIDANCE.to_string(),
            });
        }

        let meeting_id = &requests[0].meeting_id;
        for request in requests {
            if request.meeting_id != *meeting_id {
                return Err(TranscriptionError::UnsupportedAudioInput {
                    path: request.audio_path.clone(),
                    guidance: "Bundled transcription requires all audio artifacts to belong to the same meeting.".to_string(),
                });
            }
            validate_wav_input(&request.audio_path)?;
        }

        let source_artifact_sha256 = bundled_source_artifact_sha256(requests);
        let model_run_id = model_run_id(
            self.backend.provider(),
            &self.model_name,
            meeting_id,
            &source_artifact_sha256,
        );
        let transcript_version_id = format!("{model_run_id}-v1");
        let mut bundle_segments = Vec::new();
        for (source_index, request) in requests.iter().enumerate() {
            let mut backend_segments = self
                .backend
                .transcribe(&self.model_path, &request.audio_path)?;
            backend_segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
            for (segment_index, segment) in backend_segments.into_iter().enumerate() {
                bundle_segments.push(BundleSegment {
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text: segment.text,
                    source_channel: request.source_channel,
                    source_index,
                    segment_index,
                });
            }
        }
        bundle_segments.sort_by_key(|segment| {
            (
                segment.start_ms,
                segment.end_ms,
                source_channel_rank(segment.source_channel),
                segment.source_index,
                segment.segment_index,
            )
        });

        let segments = bundle_segments
            .into_iter()
            .enumerate()
            .map(|(index, segment)| {
                TranscriptSegment::with_metadata(
                    format!("{transcript_version_id}-segment-{index}"),
                    meeting_id.clone(),
                    segment.start_ms,
                    segment.end_ms,
                    segment.text,
                    segment.source_channel,
                    model_run_id.clone(),
                    transcript_version_id.clone(),
                )
            })
            .collect();

        Ok(TranscriptionDocument {
            provider: self.backend.provider().to_string(),
            model_name: self.model_name.clone(),
            model_run_id,
            transcript_version_id,
            source_artifact_sha256,
            segments,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BundleSegment {
    start_ms: u64,
    end_ms: u64,
    text: String,
    source_channel: SourceChannel,
    source_index: usize,
    segment_index: usize,
}

#[cfg(feature = "whisper-rs")]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RealWhisperBackend;

#[cfg(feature = "whisper-rs")]
impl WhisperBackend for RealWhisperBackend {
    fn provider(&self) -> &'static str {
        "local-whisper"
    }

    fn transcribe(
        &self,
        model_path: &Path,
        audio_path: &Path,
    ) -> Result<Vec<WhisperBackendSegment>, TranscriptionError> {
        let samples = load_wav_samples_16khz_mono(audio_path)?;
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
                .map_err(|error| backend_failed(self.provider(), error))?;
        let mut state = context
            .create_state()
            .map_err(|error| backend_failed(self.provider(), error))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, &samples)
            .map_err(|error| backend_failed(self.provider(), error))?;

        state
            .as_iter()
            .map(|segment| {
                let text = segment
                    .to_str_lossy()
                    .map_err(|error| backend_failed(self.provider(), error))?
                    .into_owned();
                Ok(WhisperBackendSegment::new(
                    centiseconds_to_ms(segment.start_timestamp()),
                    centiseconds_to_ms(segment.end_timestamp()),
                    text,
                ))
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WhisperSmokeStatus {
    Skipped { reason: String },
    Unavailable { reason: String },
    Passed { segment_count: usize },
    Failed { message: String },
}

impl WhisperSmokeStatus {
    pub fn was_run(&self) -> bool {
        matches!(self, Self::Passed { .. } | Self::Failed { .. })
    }

    pub fn passed(&self) -> bool {
        matches!(self, Self::Passed { .. })
    }
}

pub fn run_optional_real_whisper_smoke(
    model_path: Option<impl Into<PathBuf>>,
    audio_path: Option<impl Into<PathBuf>>,
) -> WhisperSmokeStatus {
    let Some(model_path) = model_path.map(Into::into) else {
        return WhisperSmokeStatus::Skipped {
            reason: WHISPER_SMOKE_MODEL_AND_WAV_GUIDANCE.to_string(),
        };
    };
    let Some(audio_path) = audio_path.map(Into::into) else {
        return WhisperSmokeStatus::Skipped {
            reason: WHISPER_SMOKE_WAV_GUIDANCE.to_string(),
        };
    };

    run_real_whisper_smoke(model_path, audio_path)
}

pub fn run_optional_real_whisper_smoke_from_env() -> WhisperSmokeStatus {
    run_optional_real_whisper_smoke(
        std::env::var_os("CURIOSITY_WHISPER_MODEL").map(PathBuf::from),
        std::env::var_os("CURIOSITY_WHISPER_WAV").map(PathBuf::from),
    )
}

pub trait LocalTranscriber {
    fn transcribe_fixture(
        &self,
        fixture: &AudioFixture,
    ) -> TranscriptionResult<TranscriptionDocument>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeLocalTranscriber {
    provider: String,
    model_name: String,
    version: u32,
}

impl FakeLocalTranscriber {
    pub fn new(provider: impl Into<String>, model_name: impl Into<String>, version: u32) -> Self {
        Self {
            provider: provider.into(),
            model_name: model_name.into(),
            version,
        }
    }
}

impl LocalTranscriber for FakeLocalTranscriber {
    fn transcribe_fixture(
        &self,
        fixture: &AudioFixture,
    ) -> TranscriptionResult<TranscriptionDocument> {
        let model_run_id = format!(
            "run-{}-{}-{}",
            fixture.meeting_id, self.provider, self.model_name
        );
        let transcript_version_id = format!("{model_run_id}-v{}", self.version);
        let mut lines = fixture.lines.clone();
        lines.sort_by_key(|line| (line.start_ms, line.end_ms));
        let segments = lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                TranscriptSegment::with_metadata(
                    format!("{transcript_version_id}-segment-{index}"),
                    fixture.meeting_id.clone(),
                    line.start_ms,
                    line.end_ms,
                    line.text,
                    line.source_channel,
                    model_run_id.clone(),
                    transcript_version_id.clone(),
                )
            })
            .collect();
        Ok(TranscriptionDocument {
            provider: self.provider.clone(),
            model_name: self.model_name.clone(),
            model_run_id,
            transcript_version_id,
            source_artifact_sha256: fixture.source_artifact_sha256.clone(),
            segments,
        })
    }
}

pub fn export_markdown(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .map(|segment| format!("- [{}] {}", format_clock(segment.start_ms), segment.text))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn export_json(segments: &[TranscriptSegment]) -> TranscriptionResult<String> {
    let values = segments
        .iter()
        .map(|segment| {
            serde_json::json!({
                "id": segment.id,
                "start_ms": segment.start_ms,
                "end_ms": segment.end_ms,
                "source_channel": format!("{:?}", segment.source_channel),
                "text": segment.text,
                "original_text": segment.original_text,
                "model_run_id": segment.model_run_id,
                "transcript_version_id": segment.transcript_version_id,
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::to_string_pretty(&values)?)
}

pub fn export_srt(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                index + 1,
                format_srt_time(segment.start_ms),
                format_srt_time(segment.end_ms),
                segment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_clock(ms: u64) -> String {
    let total_seconds = ms / 1_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn format_srt_time(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

fn validate_wav_input(path: &Path) -> Result<(), TranscriptionError> {
    load_wav_samples_16khz_mono(path).map(|_| ())
}

fn load_wav_samples_16khz_mono(path: &Path) -> Result<Vec<f32>, TranscriptionError> {
    let metadata = fs::metadata(path).map_err(|_| audio_unavailable(path))?;
    if !metadata.is_file() {
        return Err(audio_unavailable(path));
    }

    let reader = WavReader::open(path).map_err(|error| wav_error(path, error))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err(unsupported_audio(path, "WAV has no audio channels"));
    }
    if spec.sample_rate == 0 {
        return Err(unsupported_audio(path, "WAV has no sample rate"));
    }
    if spec.bits_per_sample == 0 || spec.bits_per_sample > 32 {
        return Err(unsupported_audio(
            path,
            "WAV bit depth must be between 1 and 32 bits",
        ));
    }

    let channels = usize::from(spec.channels);
    let interleaved = read_wav_samples(path, reader, spec)?;
    let mono = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect::<Vec<_>>();

    Ok(resample_to_16khz_mono(mono, spec.sample_rate))
}

fn read_wav_samples<R: std::io::Read>(
    path: &Path,
    reader: WavReader<R>,
    spec: hound::WavSpec,
) -> Result<Vec<f32>, TranscriptionError> {
    match spec.sample_format {
        SampleFormat::Int => {
            let scale = 2_f32.powi(i32::from(spec.bits_per_sample.saturating_sub(1)));
            reader
                .into_samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|sample| (sample as f32 / scale).clamp(-1.0, 1.0))
                        .map_err(|error| wav_error(path, error))
                })
                .collect()
        }
        SampleFormat::Float => {
            if spec.bits_per_sample != 32 {
                return Err(unsupported_audio(
                    path,
                    "IEEE-float WAV input must use 32-bit samples",
                ));
            }
            reader
                .into_samples::<f32>()
                .map(|sample| sample.map_err(|error| wav_error(path, error)))
                .collect()
        }
    }
}

fn resample_to_16khz_mono(samples: Vec<f32>, sample_rate: u32) -> Vec<f32> {
    const WHISPER_SAMPLE_RATE: u32 = 16_000;

    if sample_rate == WHISPER_SAMPLE_RATE || samples.is_empty() {
        return samples;
    }

    let output_len = ((samples.len() as u64 * u64::from(WHISPER_SAMPLE_RATE))
        / u64::from(sample_rate))
    .max(1) as usize;
    let ratio = sample_rate as f64 / WHISPER_SAMPLE_RATE as f64;
    let mut resampled = Vec::with_capacity(output_len);

    for output_index in 0..output_len {
        let source_position = output_index as f64 * ratio;
        let lower_index = source_position.floor() as usize;
        let upper_index = (lower_index + 1).min(samples.len() - 1);
        let fraction = (source_position - lower_index as f64) as f32;
        let lower = samples[lower_index.min(samples.len() - 1)];
        let upper = samples[upper_index];
        resampled.push(lower + (upper - lower) * fraction);
    }

    resampled
}

fn wav_error(path: &Path, error: hound::Error) -> TranscriptionError {
    match error {
        hound::Error::IoError(_) => audio_unavailable(path),
        other => unsupported_audio(path, &other.to_string()),
    }
}

fn audio_unavailable(path: &Path) -> TranscriptionError {
    TranscriptionError::AudioInputUnavailable {
        path: path.to_path_buf(),
        guidance: AUDIO_UNAVAILABLE_GUIDANCE.to_string(),
    }
}

fn unsupported_audio(path: &Path, reason: &str) -> TranscriptionError {
    TranscriptionError::UnsupportedAudioInput {
        path: path.to_path_buf(),
        guidance: format!("{reason}. {UNSUPPORTED_AUDIO_GUIDANCE}"),
    }
}

fn model_run_id(provider: &str, model_name: &str, meeting_id: &str, source_hash: &str) -> String {
    let raw_identity = format!("{provider}\0{model_name}\0{meeting_id}\0{source_hash}");
    let identity_hash = stable_hex_hash(raw_identity.as_bytes());

    format!(
        "run-{}-{}-{}-{}-{}",
        sanitize_id(meeting_id),
        sanitize_id(provider),
        sanitize_id(model_name),
        sanitize_id(source_hash),
        identity_hash
    )
}

fn bundled_source_artifact_sha256(requests: &[WhisperTranscriptionRequest]) -> String {
    let mut hasher = Sha256::new();
    for request in requests {
        hasher.update(request.meeting_id.as_bytes());
        hasher.update([0]);
        hasher.update(source_channel_name(request.source_channel).as_bytes());
        hasher.update([0]);
        hasher.update(request.source_artifact_sha256.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn source_channel_name(source_channel: SourceChannel) -> &'static str {
    match source_channel {
        SourceChannel::Microphone => "Microphone",
        SourceChannel::System => "System",
        SourceChannel::Mixed => "Mixed",
        SourceChannel::Imported => "Imported",
    }
}

fn source_channel_rank(source_channel: SourceChannel) -> u8 {
    match source_channel {
        SourceChannel::Microphone => 0,
        SourceChannel::System => 1,
        SourceChannel::Mixed => 2,
        SourceChannel::Imported => 3,
    }
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn stable_hex_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(feature = "whisper-rs")]
fn centiseconds_to_ms(centiseconds: i64) -> u64 {
    u64::try_from(centiseconds).unwrap_or(0).saturating_mul(10)
}

#[cfg(feature = "whisper-rs")]
fn backend_failed(provider: &str, error: impl fmt::Display) -> TranscriptionError {
    TranscriptionError::BackendFailed {
        provider: provider.to_string(),
        message: error.to_string(),
    }
}

#[cfg(feature = "whisper-rs")]
fn run_real_whisper_smoke(model_path: PathBuf, audio_path: PathBuf) -> WhisperSmokeStatus {
    match RealWhisperBackend.transcribe(&model_path, &audio_path) {
        Ok(segments) => WhisperSmokeStatus::Passed {
            segment_count: segments.len(),
        },
        Err(error) => WhisperSmokeStatus::Failed {
            message: error.to_string(),
        },
    }
}

#[cfg(not(feature = "whisper-rs"))]
fn run_real_whisper_smoke(_model_path: PathBuf, _audio_path: PathBuf) -> WhisperSmokeStatus {
    WhisperSmokeStatus::Unavailable {
        reason: concat!(
            "Compile curiosity-transcription with --features whisper-rs to run the real ",
            "local Whisper smoke."
        )
        .to_string(),
    }
}
