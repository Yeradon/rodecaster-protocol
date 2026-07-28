# rodecaster-protocol

A pure-Rust protocol library for the RØDECaster Pro II and RØDECaster Duo.

This crate provides typed commands, events, and session state management over JUCE's `ValueTreeSynchroniser` binary protocol. It is self-contained, dependency-free (`std` only), and transport-agnostic: it handles byte serialization and deserialization without opening sockets or device handles directly.

## Features

- **Typed Commands and Events**: Faders, mutes, solos, audio routing matrix, sound pads, and channel DSP parameters.
- **Dynamic Layout Discovery**: Reads the device state tree on connection to determine model, fader count, and channel mappings at runtime.
- **Transport Codecs**: Byte framing for USB HID reports (`usb`) and TCP streams (`frame`).
- **Low-Level Codecs**: Direct access to underlying `ValueTree` and `juce::var` parsers.
- **Zero Dependencies**: Pure `std` Rust.

## Getting Started

```rust
use rodecaster_protocol::{Command, Fader, ProtocolSession, SessionUpdate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = ProtocolSession::new();

    // Ingest binary payloads received over USB HID or TCP
    let raw_payload: &[u8] = &[];
    if let Ok(update) = session.ingest(raw_payload) {
        match update {
            SessionUpdate::Ready { initial_events } => {
                let caps = session.capabilities().unwrap();
                println!("Connected to {} (firmware {:?})", caps.model(), caps.firmware());
                println!("Loaded {} initial state parameters", initial_events.len());
            }
            SessionUpdate::Event(event) => {
                println!("Event: {event:?}");
            }
            SessionUpdate::NeedsFullSync => {
                eprintln!("Layout changed, resync required");
            }
        }
    }

    // Encode commands to wire payloads
    if session.is_ready() {
        let command = Command::SetFaderMute {
            fader: Fader::Physical1,
            mute: true,
        };
        let payloads = session.encode(&command)?;
        for payload in payloads {
            // Write payload via your transport (USB or TCP)
            let _ = payload;
        }
    }

    Ok(())
}
```

## Transports

The protocol payload is identical across connection types; only the outer framing differs:

- **USB HID** ([`usb`](src/usb.rs)): For USB device connections. Handles report IDs and 64-byte report chunking.
- **TCP Stream** ([`frame`](src/frame.rs)): For network connections. Frames messages with a 4-byte magic number (`0xF2B49E2C`) and a 4-byte length prefix.

## JUCE Wire Format

Values inside the protocol use JUCE's binary `var` serialization:

| Marker | Type       | Payload Details                       |
|--------|------------|---------------------------------------|
| `0x01` | Int        | 4 bytes, little-endian                |
| `0x02` | Bool true  | (none)                                |
| `0x03` | Bool false | (none)                                |
| `0x04` | Double     | 8 bytes, IEEE 754                     |
| `0x05` | String     | UTF-8, null-terminated                |
| `0x06` | Int64      | 8 bytes, little-endian                |
| `0x07` | Array      | Compressed-int count, then N values   |
| `0x08` | Binary     | Compressed-int length, then raw bytes |
| `0x09` | Undefined  | (none)                                |

## License

MIT
