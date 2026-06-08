/*!
conversation-codec: JSONL save/load for LLM conversation messages.

One JSON object per line — appends are cheap and partial files are
recoverable. Optional per-message redaction via a callback.

```rust
use conversation_codec::Codec;
use serde_json::json;

let messages = vec![
    json!({"role": "user", "content": "hello"}),
    json!({"role": "assistant", "content": "hi there"}),
];

let dir = std::env::temp_dir();
let path = dir.join("conv_doctest.jsonl");

let codec = Codec::new();
codec.save(&messages, &path).unwrap();
let loaded = codec.load(&path).unwrap();

assert_eq!(loaded.len(), 2);
assert_eq!(loaded[0]["role"], json!("user"));
std::fs::remove_file(&path).ok();
```
*/

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Arc;

// ---- errors ---------------------------------------------------------------

#[derive(Debug)]
pub enum CodecError {
    Io(std::io::Error),
    /// A line could not be parsed as JSON.
    Decode {
        line: usize,
        message: String,
    },
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::Io(e) => write!(f, "IO error: {e}"),
            CodecError::Decode { line, message } => {
                write!(f, "decode error at line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodecError::Io(e) => Some(e),
            CodecError::Decode { .. } => None,
        }
    }
}

impl From<std::io::Error> for CodecError {
    fn from(e: std::io::Error) -> Self {
        CodecError::Io(e)
    }
}

// ---- Codec ----------------------------------------------------------------

type RedactFn = Arc<dyn Fn(Value) -> Value + Send + Sync>;

/// JSONL conversation codec.
///
/// Use `Codec::new()` for plain JSONL. Pass a redact callback via
/// `Codec::with_redact` to transform each message before encoding.
#[derive(Clone, Default)]
pub struct Codec {
    redact: Option<RedactFn>,
}

impl Codec {
    /// Plain JSONL codec, no redaction.
    pub fn new() -> Self {
        Self::default()
    }

    /// Codec that applies `redact` to each message before encoding.
    pub fn with_redact(redact: impl Fn(Value) -> Value + Send + Sync + 'static) -> Self {
        Self {
            redact: Some(Arc::new(redact)),
        }
    }

    // ---- encode / decode (single message) ----------------------------

    fn encode(&self, msg: Value) -> String {
        let processed = if let Some(r) = &self.redact {
            r(msg)
        } else {
            msg
        };
        serde_json::to_string(&processed).unwrap_or_else(|_| "{}".into())
    }

    fn decode(line: &str, lineno: usize) -> Result<Value, CodecError> {
        serde_json::from_str(line).map_err(|e| CodecError::Decode {
            line: lineno,
            message: e.to_string(),
        })
    }

    // ---- save / load -------------------------------------------------

    /// Write messages to a JSONL file. Creates parent dirs; overwrites.
    pub fn save(&self, messages: &[Value], path: impl AsRef<Path>) -> Result<(), CodecError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut f = std::fs::File::create(path)?;
        for msg in messages {
            let line = self.encode(msg.clone());
            f.write_all(line.as_bytes())?;
            f.write_all(b"\n")?;
        }
        Ok(())
    }

    /// Append messages to an existing JSONL file (creates if absent).
    pub fn append(&self, messages: &[Value], path: impl AsRef<Path>) -> Result<(), CodecError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        for msg in messages {
            let line = self.encode(msg.clone());
            f.write_all(line.as_bytes())?;
            f.write_all(b"\n")?;
        }
        Ok(())
    }

    /// Load all messages from a JSONL file. Skips blank lines.
    pub fn load(&self, path: impl AsRef<Path>) -> Result<Vec<Value>, CodecError> {
        let f = std::fs::File::open(path.as_ref())?;
        let reader = BufReader::new(f);
        let mut out = Vec::new();
        let mut lineno = 0usize;
        for raw in reader.lines() {
            lineno += 1;
            let line = raw?;
            if line.trim().is_empty() {
                continue;
            }
            out.push(Self::decode(&line, lineno)?);
        }
        Ok(out)
    }

    /// Decode a single JSONL line.
    pub fn decode_line(line: &str) -> Result<Value, CodecError> {
        Self::decode(line, 0)
    }

    /// Encode a single message to a JSONL line (applies redact if set).
    pub fn encode_line(&self, msg: Value) -> String {
        self.encode(msg)
    }
}

impl std::fmt::Debug for Codec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Codec {{ redact: {} }}", self.redact.is_some())
    }
}

// ---- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(name)
    }

    fn cleanup(p: &std::path::Path) {
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn save_and_load_roundtrip() {
        let msgs = vec![
            json!({"role": "user", "content": "hi"}),
            json!({"role": "assistant", "content": "hello"}),
        ];
        let p = tmp_path("codec_test_roundtrip.jsonl");
        let c = Codec::new();
        c.save(&msgs, &p).unwrap();
        let loaded = c.load(&p).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["role"], json!("user"));
        assert_eq!(loaded[1]["content"], json!("hello"));
        cleanup(&p);
    }

    #[test]
    fn append_adds_messages() {
        let p = tmp_path("codec_test_append.jsonl");
        let c = Codec::new();
        c.save(&[json!({"role": "user", "content": "1"})], &p)
            .unwrap();
        c.append(&[json!({"role": "assistant", "content": "2"})], &p)
            .unwrap();
        let loaded = c.load(&p).unwrap();
        assert_eq!(loaded.len(), 2);
        cleanup(&p);
    }

    #[test]
    fn redact_applied_on_save() {
        let c = Codec::with_redact(|mut msg| {
            if let Some(obj) = msg.as_object_mut() {
                obj.insert("redacted".to_owned(), json!(true));
            }
            msg
        });
        let msgs = vec![json!({"role": "user", "content": "secret"})];
        let p = tmp_path("codec_test_redact.jsonl");
        c.save(&msgs, &p).unwrap();
        let loaded = Codec::new().load(&p).unwrap(); // load without redact
        assert_eq!(loaded[0]["redacted"], json!(true));
        cleanup(&p);
    }

    #[test]
    fn redact_strips_key() {
        let c = Codec::with_redact(|mut msg| {
            if let Some(obj) = msg.as_object_mut() {
                obj.remove("content");
            }
            msg
        });
        let msgs = vec![json!({"role": "user", "content": "private"})];
        let p = tmp_path("codec_test_redact_strip.jsonl");
        c.save(&msgs, &p).unwrap();
        let loaded = Codec::new().load(&p).unwrap();
        assert!(!loaded[0].as_object().unwrap().contains_key("content"));
        cleanup(&p);
    }

    #[test]
    fn load_skips_blank_lines() {
        let p = tmp_path("codec_test_blanks.jsonl");
        std::fs::write(
            &p,
            r#"{"role":"user"}

{"role":"assistant"}
"#,
        )
        .unwrap();
        let c = Codec::new();
        let loaded = c.load(&p).unwrap();
        assert_eq!(loaded.len(), 2);
        cleanup(&p);
    }

    #[test]
    fn bad_json_line_decode_error() {
        let p = tmp_path("codec_test_bad.jsonl");
        std::fs::write(&p, "not json\n").unwrap();
        let c = Codec::new();
        let err = c.load(&p).unwrap_err();
        assert!(matches!(err, CodecError::Decode { .. }));
        cleanup(&p);
    }

    #[test]
    fn save_overwrite() {
        let p = tmp_path("codec_test_overwrite.jsonl");
        let c = Codec::new();
        c.save(&[json!({"a": 1}), json!({"b": 2})], &p).unwrap();
        c.save(&[json!({"c": 3})], &p).unwrap();
        let loaded = c.load(&p).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["c"], json!(3));
        cleanup(&p);
    }

    #[test]
    fn empty_save_empty_load() {
        let p = tmp_path("codec_test_empty.jsonl");
        let c = Codec::new();
        c.save(&[], &p).unwrap();
        let loaded = c.load(&p).unwrap();
        assert!(loaded.is_empty());
        cleanup(&p);
    }

    #[test]
    fn encode_line_applies_redact() {
        let c = Codec::with_redact(|mut v| {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("tagged".to_owned(), json!(true));
            }
            v
        });
        let line = c.encode_line(json!({"x": 1}));
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["tagged"], json!(true));
    }

    #[test]
    fn decode_line_parses_json() {
        let v = Codec::decode_line(r#"{"role":"user"}"#).unwrap();
        assert_eq!(v["role"], json!("user"));
    }

    #[test]
    fn decode_line_bad_returns_err() {
        assert!(Codec::decode_line("bad").is_err());
    }

    #[test]
    fn nonobject_messages_saved() {
        let p = tmp_path("codec_test_array_msg.jsonl");
        let c = Codec::new();
        let msgs = vec![json!([1, 2, 3]), json!("plain string"), json!(42)];
        c.save(&msgs, &p).unwrap();
        let loaded = c.load(&p).unwrap();
        assert_eq!(loaded[0], json!([1, 2, 3]));
        assert_eq!(loaded[1], json!("plain string"));
        cleanup(&p);
    }
}
