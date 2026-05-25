# conversation-codec

JSONL save/load for LLM conversation messages. One JSON object per line — appends are cheap and partial files are recoverable.

## Usage

```rust
use conversation_codec::Codec;
use serde_json::json;

let messages = vec![
    json!({"role": "user", "content": "hello"}),
    json!({"role": "assistant", "content": "hi"}),
];

let codec = Codec::new();
codec.save(&messages, "conv.jsonl").unwrap();
let loaded = codec.load("conv.jsonl").unwrap();
assert_eq!(loaded.len(), 2);

// Optional per-message redaction
let c = Codec::with_redact(|mut msg| {
    if let Some(obj) = msg.as_object_mut() {
        obj.remove("api_key");
    }
    msg
});
```

## License

MIT OR Apache-2.0
