use std::fs;
use std::path::PathBuf;

use curiosity_store::{AppSettings, Store, DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL};

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
}
