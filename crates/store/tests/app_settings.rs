use std::fs;
use std::path::PathBuf;

use curiosity_domain::RawAudioRetentionPolicy;
use curiosity_store::{
    AppSettings, OllamaConnectionTestEvidence, Store, WhisperPathTestEvidence,
    DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL,
};
use rusqlite::{params, Connection};
use serde_json::json;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-store-settings-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn app_settings_default_to_local_ollama_and_empty_optional_paths() {
    let root = test_root("defaults");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");

    let settings = store.app_settings().expect("read settings");

    assert_eq!(
        settings,
        AppSettings {
            whisper_model_path: String::new(),
            ollama_base_url: DEFAULT_OLLAMA_BASE_URL.to_string(),
            ollama_model: DEFAULT_OLLAMA_MODEL.to_string(),
            export_directory: None,
            raw_audio_retention_policy: RawAudioRetentionPolicy::Retain,
            whisper_path_test_evidence: None,
            ollama_connection_test_evidence: None,
        }
    );
}

#[test]
fn app_settings_persist_whisper_and_analysis_settings_across_store_reopen() {
    let root = test_root("persist");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate");

    store
        .save_whisper_model_path("/models/ggml-base.en.bin")
        .expect("save whisper path");
    store
        .save_analysis_settings("http://127.0.0.1:11435", "gemma4:31b")
        .expect("save analysis settings");
    drop(store);

    let reopened = Store::open(&db_path, root).expect("reopen store");
    reopened.migrate().expect("migrate reopened store");
    let settings = reopened.app_settings().expect("read settings");

    assert_eq!(settings.whisper_model_path, "/models/ggml-base.en.bin");
    assert_eq!(settings.ollama_base_url, "http://127.0.0.1:11435");
    assert_eq!(settings.ollama_model, "gemma4:31b");
    assert_eq!(settings.export_directory, None);
    assert_eq!(
        settings.raw_audio_retention_policy,
        RawAudioRetentionPolicy::Retain
    );
    assert_eq!(settings.whisper_path_test_evidence, None);
    assert_eq!(settings.ollama_connection_test_evidence, None);
}

#[test]
fn app_settings_persist_setup_test_evidence_and_clear_only_mismatched_settings() {
    let root = test_root("setup-evidence");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate");
    let whisper_evidence = WhisperPathTestEvidence {
        tested_path: "/models/ggml-base.en.bin".to_string(),
        tested_at_ms: 1_700_000_001_000,
        state: "Valid".to_string(),
        file_size_bytes: Some(16),
        sha256: Some(
            "2fb703c1815700a864ff2bbc42767fd52dc5b77635f0dfc132860420b8a94acf".to_string(),
        ),
        failure_detail: None,
    };
    let ollama_evidence = OllamaConnectionTestEvidence {
        base_url: "http://127.0.0.1:11434".to_string(),
        requested_model: "qwen3.6:27b".to_string(),
        tested_at_ms: 1_700_000_002_000,
        state: "Available".to_string(),
        selected_local_model_tag: Some("qwen3.6:27b".to_string()),
        installed_local_models: Some(vec!["gemma4:31b".to_string(), "qwen3.6:27b".to_string()]),
        pull_command: None,
        failure_detail: None,
    };

    store
        .save_whisper_path_test_evidence(&whisper_evidence)
        .expect("save whisper evidence");
    store
        .save_ollama_connection_test_evidence(&ollama_evidence)
        .expect("save ollama evidence");
    drop(store);

    let reopened = Store::open(&db_path, root).expect("reopen store");
    reopened.migrate().expect("migrate reopened store");
    let settings = reopened.app_settings().expect("settings with evidence");
    assert_eq!(
        settings.whisper_path_test_evidence,
        Some(whisper_evidence.clone())
    );
    assert_eq!(
        settings.ollama_connection_test_evidence,
        Some(ollama_evidence.clone())
    );

    reopened
        .save_whisper_model_path("/models/ggml-base.en.bin")
        .expect("save same whisper path");
    reopened
        .save_analysis_settings("http://127.0.0.1:11434", "qwen3.6:27b")
        .expect("save same analysis settings");
    let kept = reopened.app_settings().expect("settings kept evidence");
    assert_eq!(kept.whisper_path_test_evidence, Some(whisper_evidence));
    assert_eq!(kept.ollama_connection_test_evidence, Some(ollama_evidence));

    reopened
        .save_whisper_model_path("/models/other.bin")
        .expect("save different whisper path");
    assert_eq!(
        reopened
            .app_settings()
            .expect("settings after whisper mismatch")
            .whisper_path_test_evidence,
        None
    );

    reopened
        .save_analysis_settings("http://127.0.0.1:11435", "qwen3.6:27b")
        .expect("save different analysis settings");
    assert_eq!(
        reopened
            .app_settings()
            .expect("settings after ollama mismatch")
            .ollama_connection_test_evidence,
        None
    );
}

#[test]
fn app_settings_ignore_and_clear_malformed_setup_test_evidence() {
    let root = test_root("malformed-setup-evidence");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate");
    drop(store);

    let conn = Connection::open(&db_path).expect("open sqlite");
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
        params!["whisper_path_test_evidence", "{not valid json"],
    )
    .expect("insert malformed whisper evidence");
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
        params!["ollama_connection_test_evidence", "{not valid json"],
    )
    .expect("insert malformed ollama evidence");
    drop(conn);

    let reopened = Store::open(&db_path, root).expect("reopen store");
    reopened.migrate().expect("migrate reopened store");
    let settings = reopened
        .app_settings()
        .expect("malformed optional evidence should not fail settings");
    assert_eq!(settings.whisper_path_test_evidence, None);
    assert_eq!(settings.ollama_connection_test_evidence, None);

    reopened
        .save_whisper_model_path("/models/ggml-base.en.bin")
        .expect("save should clear malformed whisper evidence");
    reopened
        .save_analysis_settings("http://127.0.0.1:11435", "gemma4:31b")
        .expect("save should clear malformed ollama evidence");

    let conn = Connection::open(&db_path).expect("open sqlite after save");
    let evidence_count: i64 = conn
        .query_row(
            "
            SELECT COUNT(*)
            FROM app_settings
            WHERE key IN (?1, ?2)
            ",
            params![
                "whisper_path_test_evidence",
                "ollama_connection_test_evidence"
            ],
            |row| row.get(0),
        )
        .expect("count setup evidence settings");
    assert_eq!(evidence_count, 0);
}

#[test]
fn app_settings_ignore_and_clear_contract_invalid_setup_test_evidence() {
    let root = test_root("contract-invalid-setup-evidence");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate");
    drop(store);

    let conn = Connection::open(&db_path).expect("open sqlite");
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
        params![
            "whisper_path_test_evidence",
            json!({
                "testedPath": "/models/ggml-base.en.bin",
                "testedAtMs": 1_700_000_001_000_u64,
                "state": "Bogus",
                "fileSizeBytes": 16,
                "sha256": "not-a-sha256",
                "failureDetail": null
            })
            .to_string()
        ],
    )
    .expect("insert contract-invalid whisper evidence");
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
        params![
            "ollama_connection_test_evidence",
            json!({
                "baseUrl": "http://127.0.0.1:11434",
                "requestedModel": "qwen3.6:27b",
                "testedAtMs": 1_700_000_002_000_u64,
                "state": "Maybe",
                "selectedLocalModelTag": null,
                "installedLocalModels": [],
                "pullCommand": null,
                "failureDetail": null
            })
            .to_string()
        ],
    )
    .expect("insert contract-invalid ollama evidence");
    drop(conn);

    let reopened = Store::open(&db_path, root).expect("reopen store");
    reopened.migrate().expect("migrate reopened store");
    let settings = reopened
        .app_settings()
        .expect("contract-invalid optional evidence should not fail settings");
    assert_eq!(settings.whisper_path_test_evidence, None);
    assert_eq!(settings.ollama_connection_test_evidence, None);

    reopened
        .save_whisper_model_path("/models/ggml-base.en.bin")
        .expect("same whisper path save should clear invalid evidence");
    reopened
        .save_analysis_settings("http://127.0.0.1:11434", "qwen3.6:27b")
        .expect("same ollama settings save should clear invalid evidence");

    let conn = Connection::open(&db_path).expect("open sqlite after save");
    let evidence_count: i64 = conn
        .query_row(
            "
            SELECT COUNT(*)
            FROM app_settings
            WHERE key IN (?1, ?2)
            ",
            params![
                "whisper_path_test_evidence",
                "ollama_connection_test_evidence"
            ],
            |row| row.get(0),
        )
        .expect("count setup evidence settings");
    assert_eq!(evidence_count, 0);
}

#[test]
fn app_settings_persist_supported_raw_audio_retention_default_across_store_reopen() {
    let root = test_root("retention-persist");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate");

    store
        .save_raw_audio_retention_policy("DeleteAfterTranscription")
        .expect("save retention policy");
    drop(store);

    let reopened = Store::open(&db_path, root).expect("reopen store");
    reopened.migrate().expect("migrate reopened store");
    let settings = reopened.app_settings().expect("read settings");

    assert_eq!(
        settings.raw_audio_retention_policy,
        RawAudioRetentionPolicy::DeleteAfterTranscription
    );
}

#[test]
fn app_settings_reject_unsupported_never_save_retention_default() {
    let root = test_root("retention-never-save");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");

    let error = store
        .save_raw_audio_retention_policy("NeverSave")
        .expect_err("NeverSave is not supported by this production slice");

    assert!(
        error
            .to_string()
            .contains("unsupported raw audio retention policy"),
        "unsupported policy should fail loudly: {error}"
    );
    assert_eq!(
        store
            .app_settings()
            .expect("settings after rejected save")
            .raw_audio_retention_policy,
        RawAudioRetentionPolicy::Retain
    );
}
