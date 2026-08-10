use crate::{
    frame_end_time_ms, samples_to_ms_ceil, ArtifactManifest, AudioArtifactMetadata, AudioFrame,
    CaptureConfiguration, DeviceIdentity, DeviceSnapshot, ManifestStatus, RecordingError,
    RecordingMetadata, RecoveryMetadata, StreamKind, StreamMetadata,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

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
        self.manifest.ended_at_ms = Some(ended_at_ms);
        self.finalize_active_artifacts()?;
        if let Some(missing_stream) = self.config.requested_streams().into_iter().find(|stream| {
            !self
                .manifest
                .artifacts
                .iter()
                .any(|artifact| artifact.stream == *stream)
        }) {
            self.manifest.status = ManifestStatus::Failed;
            self.manifest.recovery = Some(RecoveryMetadata {
                recoverable: false,
                reason: format!(
                    "requested {} stream produced no samples",
                    missing_stream.as_manifest_str()
                ),
            });
            self.write_manifest()?;
            return Err(RecordingError::MissingRequestedStream(missing_stream));
        }
        self.manifest.status = ManifestStatus::Complete;
        self.write_manifest()?;
        Ok(self.manifest)
    }

    pub fn fail(
        mut self,
        ended_at_ms: u64,
        reason: impl Into<String>,
    ) -> Result<ArtifactManifest, RecordingError> {
        self.manifest.ended_at_ms = Some(ended_at_ms);
        self.finalize_active_artifacts()?;
        self.manifest.status = ManifestStatus::Failed;
        self.manifest.recovery = Some(RecoveryMetadata {
            recoverable: false,
            reason: reason.into(),
        });
        self.write_manifest()?;
        Ok(self.manifest)
    }

    fn finalize_active_artifacts(&mut self) -> Result<(), RecordingError> {
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
        Ok(())
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
                "artifact={},{},Writing,{}",
                stream.as_manifest_str(),
                artifact.file_name,
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
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
