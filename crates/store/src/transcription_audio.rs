use rusqlite::params;

use super::{artifact_kinds_satisfy_recording_source, Store, StoreResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptionAudioArtifact {
    pub artifact_id: String,
    pub recording_session_id: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
}

impl Store {
    pub fn completed_wav_artifact_for_transcription(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Option<TranscriptionAudioArtifact>> {
        Ok(self
            .completed_wav_artifacts_for_transcription(meeting_id)?
            .into_iter()
            .next())
    }

    pub fn completed_wav_artifacts_for_transcription(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Vec<TranscriptionAudioArtifact>> {
        let meeting_path_prefix = format!("meetings/{meeting_id}/");
        let mut stmt = self.conn.prepare(
            "
            SELECT
                audio_artifacts.id,
                audio_artifacts.recording_session_id,
                audio_artifacts.kind,
                audio_artifacts.path,
                audio_artifacts.sha256,
                recording_sessions.source
            FROM audio_artifacts
            JOIN recording_sessions
              ON recording_sessions.id = audio_artifacts.recording_session_id
            WHERE recording_sessions.meeting_id = ?1
              AND audio_artifacts.retained = 1
              AND audio_artifacts.write_status = 'Complete'
              AND audio_artifacts.tombstoned = 0
              AND recording_sessions.status IN ('Complete', 'Recovered')
              AND lower(audio_artifacts.path) LIKE '%.wav'
            ORDER BY recording_sessions.started_at_ms DESC,
                     audio_artifacts.id ASC
            ",
        )?;
        let artifacts = stmt
            .query_map(params![meeting_id], |row| {
                Ok((
                    TranscriptionAudioArtifact {
                        artifact_id: row.get(0)?,
                        recording_session_id: row.get(1)?,
                        kind: row.get(2)?,
                        path: row.get(3)?,
                        sha256: row.get(4)?,
                    },
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut artifacts = artifacts
            .into_iter()
            .filter(|(artifact, _source)| {
                artifact.path.starts_with(&meeting_path_prefix)
                    && self.private_app_path(&artifact.path).is_some()
            })
            .collect::<Vec<_>>();
        let Some((first_artifact, recording_source)) = artifacts.first() else {
            return Ok(Vec::new());
        };
        let recording_session_id = first_artifact.recording_session_id.clone();
        let recording_source = recording_source.clone();
        artifacts
            .retain(|(artifact, _source)| artifact.recording_session_id == recording_session_id);
        let mut artifacts = artifacts
            .into_iter()
            .map(|(artifact, _source)| artifact)
            .collect::<Vec<_>>();
        if !transcription_artifacts_satisfy_recording_source(&recording_source, &artifacts) {
            return Ok(Vec::new());
        }
        artifacts.sort_by_key(|artifact| {
            (
                transcription_artifact_kind_rank(&artifact.kind),
                artifact.artifact_id.clone(),
            )
        });
        Ok(artifacts)
    }
}

fn transcription_artifact_kind_rank(kind: &str) -> u8 {
    match kind {
        "RawMic" => 0,
        "RawSystem" => 1,
        "Mixed" => 2,
        "Imported" => 3,
        _ => 4,
    }
}

fn transcription_artifacts_satisfy_recording_source(
    recording_source: &str,
    artifacts: &[TranscriptionAudioArtifact],
) -> bool {
    let kinds = artifacts
        .iter()
        .map(|artifact| artifact.kind.as_str())
        .collect::<Vec<_>>();
    artifact_kinds_satisfy_recording_source(recording_source, &kinds)
}
