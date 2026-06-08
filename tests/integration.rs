//! Integration tests that exercise `conversation-codec` through its public API,
//! the way a downstream crate would. These complement the in-crate unit tests
//! in `src/lib.rs` and focus on filesystem-facing edge cases.

use conversation_codec::{Codec, CodecError};
use serde_json::json;
use std::path::PathBuf;

/// Returns a unique temp path so tests can run in parallel without colliding,
/// and removes any leftover file from a previous run.
fn fresh_path(name: &str) -> PathBuf {
    // Combine PID and a per-name counter for uniqueness across parallel tests.
    let unique = format!("conv_codec_it_{}_{}_{}", std::process::id(), name, line!());
    let p = std::env::temp_dir().join(unique);
    std::fs::remove_file(&p).ok();
    p
}

#[test]
fn append_creates_file_when_absent() {
    let p = fresh_path("append_new");
    // File does not exist yet; append must create it.
    let codec = Codec::new();
    codec
        .append(&[json!({"role": "user", "content": "first"})], &p)
        .unwrap();
    let loaded = codec.load(&p).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0]["content"], json!("first"));
    std::fs::remove_file(&p).ok();
}

#[test]
fn load_missing_file_is_io_error() {
    let p = fresh_path("definitely_missing");
    std::fs::remove_file(&p).ok();
    let err = Codec::new().load(&p).unwrap_err();
    match err {
        CodecError::Io(_) => {}
        other => panic!("expected Io error, got {other:?}"),
    }
}

#[test]
fn decode_error_reports_one_based_line_number() {
    let p = fresh_path("bad_line");
    // Two valid lines, then a malformed third line.
    std::fs::write(
        &p,
        "{\"role\":\"user\"}\n{\"role\":\"assistant\"}\nnot json at all\n",
    )
    .unwrap();
    let err = Codec::new().load(&p).unwrap_err();
    match err {
        CodecError::Decode { line, .. } => assert_eq!(line, 3),
        other => panic!("expected Decode error, got {other:?}"),
    }
    std::fs::remove_file(&p).ok();
}

#[test]
fn save_creates_nested_parent_directories() {
    let base = std::env::temp_dir().join(format!("conv_codec_nested_{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    let nested = base.join("a").join("b").join("conv.jsonl");

    Codec::new()
        .save(&[json!({"k": "v"})], &nested)
        .expect("save should create parent dirs");
    assert!(nested.exists());

    let loaded = Codec::new().load(&nested).unwrap();
    assert_eq!(loaded[0]["k"], json!("v"));

    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn append_preserves_order_across_calls() {
    let p = fresh_path("order");
    let codec = Codec::new();
    for i in 0..5 {
        codec
            .append(&[json!({"i": i})], &p)
            .unwrap_or_else(|e| panic!("append {i} failed: {e}"));
    }
    let loaded = codec.load(&p).unwrap();
    let ints: Vec<i64> = loaded.iter().map(|m| m["i"].as_i64().unwrap()).collect();
    assert_eq!(ints, vec![0, 1, 2, 3, 4]);
    std::fs::remove_file(&p).ok();
}

#[test]
fn codec_display_and_source_on_decode_error() {
    // Exercises the Display impl and the std::error::Error chain.
    use std::error::Error;
    let err = Codec::decode_line("totally invalid").unwrap_err();
    let shown = err.to_string();
    assert!(
        shown.contains("decode error"),
        "unexpected Display output: {shown}"
    );
    // Decode errors have no underlying source.
    assert!(err.source().is_none());
}

#[test]
fn redact_runs_on_append_not_load() {
    let p = fresh_path("redact_append");
    let codec = Codec::with_redact(|mut msg| {
        if let Some(obj) = msg.as_object_mut() {
            obj.remove("secret");
        }
        msg
    });
    codec
        .append(&[json!({"role": "user", "secret": "shh"})], &p)
        .unwrap();
    // Load with a plain codec: the secret must already be gone from disk.
    let loaded = Codec::new().load(&p).unwrap();
    assert!(loaded[0].get("secret").is_none());
    std::fs::remove_file(&p).ok();
}
