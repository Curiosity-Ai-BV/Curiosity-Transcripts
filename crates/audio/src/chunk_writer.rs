use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::{
    frame_end_time_ms, AudioFrame, ChunkManifest, ChunkMetadata, ChunkRecoveryState, ChunkStatus,
    ManifestStatus, RecordingMetadata, RecoveryMetadata, StreamKind,
};

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
