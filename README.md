# conversation-codec

[![CI](https://github.com/MukundaKatta/conversation-codec-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/MukundaKatta/conversation-codec-rs/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/conversation-codec.svg)](https://crates.io/crates/conversation-codec)
[![Docs.rs](https://docs.rs/conversation-codec/badge.svg)](https://docs.rs/conversation-codec)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

JSONL save/load for LLM conversation messages, with optional per-message
redaction. One JSON object per line, so appends are cheap and a partial file
(e.g. after a crash) is still readable up to the last complete line.

The only dependency is [`serde_json`](https://crates.io/crates/serde_json):
messages are plain `serde_json::Value`s, so there is no fixed schema to fight
with — store whatever shape your chat API uses.

## Why JSONL?

- **Cheap appends** — adding a turn to a conversation is a single line write,
  no need to re-serialize the whole file.
- **Crash-resilient** — if a process dies mid-write, every complete line before
  the truncated one still loads. `load` reports the exact line number when a
  line fails to parse.
- **Streamable & greppable** — each message is an independent JSON document, so
  the file plays nicely with `jq`, `grep`, and line-oriented tooling.

## Installation

Add it to your `Cargo.toml`:

```toml
[dependencies]
conversation-codec = "0.1"
serde_json = "1"
```

Or with cargo:

```sh
cargo add conversation-codec serde_json
```

The crate's minimum supported Rust version (MSRV) is **1.74**.

## Usage

```rust
use conversation_codec::Codec;
use serde_json::json;

let messages = vec![
    json!({"role": "user", "content": "hello"}),
    json!({"role": "assistant", "content": "hi there"}),
];

// Plain JSONL: write, then read back.
let codec = Codec::new();
codec.save(&messages, "conv.jsonl").unwrap();

let loaded = codec.load("conv.jsonl").unwrap();
assert_eq!(loaded.len(), 2);
assert_eq!(loaded[0]["role"], json!("user"));

// Append a follow-up turn without rewriting the file.
codec
    .append(&[json!({"role": "user", "content": "thanks!"})], "conv.jsonl")
    .unwrap();
assert_eq!(codec.load("conv.jsonl").unwrap().len(), 3);
```

### Redaction

Pass a callback to `Codec::with_redact` to transform each message *before* it is
encoded. This is handy for stripping secrets, dropping large fields, or tagging
messages on the way to disk. Redaction runs on `save`, `append`, and
`encode_line` — never on `load`.

```rust
use conversation_codec::Codec;
use serde_json::json;

// Drop an `api_key` field before anything touches disk.
let codec = Codec::with_redact(|mut msg| {
    if let Some(obj) = msg.as_object_mut() {
        obj.remove("api_key");
    }
    msg
});

codec
    .save(&[json!({"role": "user", "content": "hi", "api_key": "sk-secret"})], "safe.jsonl")
    .unwrap();

// The stored message no longer contains the secret.
let loaded = Codec::new().load("safe.jsonl").unwrap();
assert!(loaded[0].get("api_key").is_none());
```

### Working with single lines

`encode_line` / `decode_line` let you handle one message at a time, for example
when streaming to a socket or an existing writer.

```rust
use conversation_codec::Codec;
use serde_json::json;

let codec = Codec::new();
let line = codec.encode_line(json!({"role": "user", "content": "ping"}));
let msg = Codec::decode_line(&line).unwrap();
assert_eq!(msg["content"], json!("ping"));
```

## API

| Item | Description |
| --- | --- |
| `Codec::new()` | Plain JSONL codec with no redaction. |
| `Codec::with_redact(f)` | Codec that runs `f: Fn(Value) -> Value` on each message before encoding. |
| `Codec::save(&msgs, path)` | Write messages to `path`, **overwriting** it. Creates parent directories. |
| `Codec::append(&msgs, path)` | Append messages to `path`, creating it if absent. |
| `Codec::load(path)` | Read all messages from `path`. Blank lines are skipped. |
| `Codec::encode_line(msg)` | Encode one message to a JSONL string (applies redaction). |
| `Codec::decode_line(line)` | Parse one JSONL line back into a `Value`. |

All fallible methods return `Result<_, CodecError>`, where `CodecError` is
either:

- `CodecError::Io(std::io::Error)` — a filesystem error (e.g. missing file on
  `load`), or
- `CodecError::Decode { line, message }` — a line that is not valid JSON, with
  the 1-based line number where decoding failed.

`Codec` is `Clone`, `Send`, and `Sync`, so a single configured codec can be
shared across threads.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
