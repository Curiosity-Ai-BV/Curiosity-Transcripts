use curiosity_domain::{SourceChannel, TranscriptSegment};

pub type TranscriptionResult<T> = Result<T, Box<dyn std::error::Error>>;

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

pub trait LocalTranscriber {
    fn transcribe_fixture(&self, fixture: &AudioFixture) -> TranscriptionResult<TranscriptionDocument>;
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
    fn transcribe_fixture(&self, fixture: &AudioFixture) -> TranscriptionResult<TranscriptionDocument> {
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
