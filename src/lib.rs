//! A pure-Rust protocol library for controlling RØDECaster Pro II and RØDECaster Duo audio consoles.
//!
//! This crate implements the binary protocol used by RØDECaster hardware and
//! desktop applications. It provides high-level typed commands and events while
//! leaving transport I/O (sockets, USB handles) under your control.
//!
//! # Architecture
//!
//! The library is organized into three distinct layers:
//!
//! 1. **Domain Layer (High-Level)**:
//!    - [`ProtocolSession`]: The main entry point for managing connection state and device lifecycle.
//!    - [`Command`]: Strongly-typed commands to mute faders, change volumes, trigger sound pads, and route audio.
//!    - [`DeviceEvent`]: Strongly-typed inbound events representing fader moves, button presses, and state updates.
//!    - [`DeviceCapabilities`]: Query physical and virtual fader counts, available input sources, and hardware model.
//!
//! 2. **Transport Framing**:
//!    - [`usb`]: Chunking and reassembly for 64-byte USB HID reports.
//!    - [`frame`]: Framing for TCP network streams using magic header and length prefix.
//!
//! 3. **Protocol Primitives (Low-Level)**:
//!    - [`valuetree`]: Parses JUCE `ValueTree` snapshots into structured [`Node`] and [`Property`] trees.
//!    - [`juce_var`]: Encodes and decodes typed [`Value`] items according to JUCE `var` binary formatting.
//!    - [`change_frame`]: Encodes and decodes granular `ValueTreeSynchroniser` change opcodes.
//!
//! # Quickstart
//!
//! Most applications interact directly with [`ProtocolSession`]:
//!
//! ```rust
//! use rodecaster_protocol::{Command, Fader, ProtocolSession, SessionUpdate};
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let mut session = ProtocolSession::new();
//!
//! // Pass raw payloads received from your transport (USB HID or TCP)
//! let raw_payload: &[u8] = &[];
//! if let Ok(update) = session.ingest(raw_payload) {
//!     match update {
//!         SessionUpdate::Ready { initial_events } => {
//!             let caps = session.capabilities().unwrap();
//!             println!("Connected to {} with {} faders", caps.model(), caps.faders().len());
//!             println!("Initial state contains {} events", initial_events.len());
//!         }
//!         SessionUpdate::Event(event) => {
//!             println!("Device event: {event:?}");
//!         }
//!         SessionUpdate::NeedsFullSync => {
//!             println!("Device topology changed; request full sync");
//!         }
//!     }
//! }
//!
//! // Send commands once the session is ready
//! if session.is_ready() {
//!     let command = Command::SetFaderMute {
//!         fader: Fader::Physical1,
//!         mute: true,
//!     };
//!     let packets = session.encode(&command)?;
//!     for packet in packets {
//!         // Send packet bytes to device via USB or TCP
//!         let _ = packet;
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod capabilities;
pub mod change_frame;
pub mod commands;
pub mod events;
pub mod frame;
pub mod juce_var;
pub mod layout;
pub mod names;
pub mod session;
pub mod trigger;
pub mod usb;
pub mod valuetree;

#[doc(hidden)]
pub mod test_fixtures;

pub use capabilities::DeviceCapabilities;
pub use change_frame::ChangeFrame;
pub use commands::Command;
pub use events::{
    decode_event, decode_event_from_frame, DeviceEvent, MixLinkDirection, MixLinkRequestOrigin,
};
pub use frame::{frame_payload, scan_frame, FrameScan, Packet, MAGIC_HEADER};
pub use juce_var::{read_value, Reader, Value};
pub use layout::Layout;
pub use names::{
    AppParam, AudioParam, BuildParam, ChannelParam, CurrentShowParam, DeviceModel, DuckerParam,
    EffectsParam, Fader, FxPresetParam, GuiParam, HeadphoneParam, InputSourceParam, MasterParam,
    MeterParam, MixMinusesParam, MixOutput, NetworkParam, OutputParam, PadParam, PadRecorderParam,
    ParseFaderError, ParseMixOutputError, ParseSourceError, PlayerParam, RadioParam, RadioRxParam,
    RadioTxParam, RcSyncMixParam, RecorderParam, RecordingParam, RecordingsParam, ShowControlParam,
    ShowParam, SipAdvancedParam, SipCallSlotsParam, SipCallingParam, SipRegistrationParam, Source,
    StorageVolumeParam, StreamerXMixPresetParam, StreamerXStreamMixParam, SystemParam, TestParam,
    ThemeParam, WifiScanResultParam,
};
pub use session::{ProtocolSession, SessionError, SessionUpdate};
pub use trigger::{TriggerOrigin, TriggerPhase};
pub use valuetree::{parse_valuetree, Node, Property};

/// Compile-time guarantee that the public types stay `Send + Sync` so
/// consumers can wrap them in `Arc<T>` and share across threads without
/// needing the crate to opt into locking. A future field with interior
/// mutability would fail this assertion at compile time.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Layout>();
    assert_send_sync::<DeviceCapabilities>();
    assert_send_sync::<ProtocolSession>();
    assert_send_sync::<SessionError>();
    assert_send_sync::<SessionUpdate>();
    assert_send_sync::<Command>();
    assert_send_sync::<DeviceEvent>();
    assert_send_sync::<ChangeFrame>();
    assert_send_sync::<Value>();
    assert_send_sync::<Node>();
    assert_send_sync::<Packet>();
};
