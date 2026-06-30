# rodecaster-protocol

A dependency-free Rust codec for the RODECaster Pro II / Duo control-wire protocol.

The RODECaster desktop app and firmware talk over a JUCE binary protocol: device
state is a `juce::ValueTreeSynchroniser` full-sync, and every value inside it is
a `juce::var` (the `VariantStreamMarker` set). This crate implements that codec in
both directions, plus the transport frame, so tools can read device state and
build commands without re-deriving the wire format from scratch.

Extracted from
[rodecaster-remote-server](https://github.com/Yeradon/rodecaster-remote-server)
to be a shared, reviewed, well-tested base for the wider RODECaster tooling
ecosystem.

## What's in the box

- **`juce_var`** — the `juce::var` codec: `Value` plus `read_value` /
  `Value::write_to_stream`, covering every marker (int, int64, bool, double,
  string, array, binary, void) and JUCE `writeCompressedInt` framing.
- **`valuetree`** — parse a `ValueTreeSynchroniser` full-sync (`0x02` header)
  into a `Node` / `Property` tree, with `to_xml` for inspection.
- **`frame`** — the transport `Packet` (`[u32 LE magic][u32 LE length][payload]`).
- **`command`** — the `RodeCommand` trait that command encoders implement.

## JUCE `var` markers

| Marker | Type       | Payload                               |
|--------|------------|---------------------------------------|
| `0x01` | Int        | 4 bytes, little-endian                |
| `0x02` | Bool true  | (none)                                |
| `0x03` | Bool false | (none)                                |
| `0x04` | Double     | 8 bytes, IEEE 754                     |
| `0x05` | String     | UTF-8, NUL-terminated                 |
| `0x06` | Int64      | 8 bytes, little-endian                |
| `0x07` | Array      | compressed-int count, then N values   |
| `0x08` | Binary     | compressed-int length, then raw bytes |
| `0x09` | Undefined  | (none)                                |

Each value is framed by `writeCompressedInt(1 + payload_len)`, then the marker
byte, then the payload.

## Usage

```rust
use rodecaster_protocol::{parse_valuetree, Value};

// Decode a device full-sync state dump into a tree, then inspect it as XML.
// `Node` also implements `Display`, so `format!("{root}")` works too.
if let Some(root) = parse_valuetree(payload) {
    print!("{}", root.to_xml(0));
}

// Encode a juce::var value to its wire bytes.
let mut buf = Vec::new();
Value::Int(42).write_to_stream(&mut buf);
```

## Status

`0.1.x`: the API may shift before `1.0`. The codec is validated against a real
~101 KB device capture (521 nodes, 4,451 properties) and byte-exact JUCE
reference frames. Dependency-free (`std` only), so adopting it never pulls a
dependency tree.

## License

MIT
