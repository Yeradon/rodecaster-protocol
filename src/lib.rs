//! RODECaster Pro II / Duo wire protocol: typed commands, typed device events,
//! and the JUCE codec they sit on.
//!
//! The device speaks `juce::ValueTreeSynchroniser` over a length-prefixed
//! transport. This crate owns the wire format once, for both directions, and
//! exposes it through three layers:
//!
//! ## Layer 1: JUCE serialization (the byte format)
//!
//! - [`juce_var`]: per-value `juce::var` codec ([`Value`], [`read_value`]).
//! - [`valuetree`]: ValueTree tree decode ([`parse_valuetree`], [`Node`]).
//! - [`frame`]: length-prefixed transport [`Packet`].
//!
//! ## Layer 2: JUCE `ValueTreeSynchroniser` change-frames
//!
//! - [`change_frame`]: decode/encode of all six JUCE change types
//!   (`propertyChanged`, `fullSync`, `childAdded`, `childRemoved`,
//!   `childMoved`, `propertyRemoved`). Path-aware, generic over property name.
//!
//! ## Layer 3: typed Rodecaster vocabulary
//!
//! - [`layout::Layout`]: per-device address discovery from a fullSync. Walks
//!   the parsed tree by node name and records where the addressable families
//!   (`PHYSICALINTERFACE`, `FADER`, `CHANNEL`, `MIX`) sit. **No hardcoded
//!   positions**: a future firmware layout shift surfaces at build time, not
//!   silently routes wrong.
//! - [`commands::Command`]: outgoing typed commands. `Command::encode(&Layout)`
//!   produces JUCE-faithful change-frame payloads, ID arithmetic lifted from
//!   the Layout, not duplicated per encoder.
//! - [`events::DeviceEvent`]: inbound typed events. [`decode_event`] resolves
//!   each wire path through a Layout to a typed Rodecaster event, falling
//!   back to [`events::DeviceEvent::Unknown`] for properties not yet typed
//!   (rather than silently dropping).
//!
//! ## Lifecycle
//!
//! ```text
//!   transport bytes ─▶ Packet::from_bytes ─▶ payload
//!   payload + Layout ─▶ decode_event ─▶ DeviceEvent
//!
//!   Command + Layout ─▶ encode ─▶ payload ─▶ Packet::new ─▶ transport bytes
//!
//!   fullSync payload ─▶ parse_valuetree + Layout::from_full_sync (rebuild on resync)
//! ```
//!
//! Hold one `Layout` per connected device; replace the whole value on each
//! fullSync. The crate is plain immutable data, `Send + Sync`, no internal
//! locking. Synchronization is the consumer's call.

pub mod change_frame;
pub mod command;
pub mod commands;
pub mod events;
pub mod frame;
pub mod juce_var;
pub mod layout;
pub mod names;
pub mod valuetree;

pub use change_frame::ChangeFrame;
pub use command::RodeCommand;
pub use commands::Command;
pub use events::{decode_event, DeviceEvent};
pub use frame::{frame_payload, scan_frame, FrameScan, Packet, MAGIC_HEADER};
pub use juce_var::{read_value, Reader, Value};
pub use layout::Layout;
pub use names::{DeviceModel, Fader, MixOutput, Source};
pub use valuetree::{parse_valuetree, Node, Property};

/// Compile-time guarantee that the public types stay `Send + Sync` so
/// consumers can wrap them in `Arc<T>` and share across threads without
/// needing the crate to opt into locking. A future field with interior
/// mutability would fail this assertion at compile time.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Layout>();
    assert_send_sync::<Command>();
    assert_send_sync::<DeviceEvent>();
    assert_send_sync::<ChangeFrame>();
    assert_send_sync::<Value>();
    assert_send_sync::<Node>();
    assert_send_sync::<Property>();
    assert_send_sync::<Packet>();
    assert_send_sync::<DeviceModel>();
    assert_send_sync::<Source>();
    assert_send_sync::<MixOutput>();
    assert_send_sync::<Fader>();
};
