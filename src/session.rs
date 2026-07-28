//! High-level session management and connection lifecycle.
//!
//! [`ProtocolSession`] is the central state machine for communicating with a
//! RØDECaster. It manages device discovery, keeps track of the active layout,
//! decodes incoming payloads into [`DeviceEvent`]s, and encodes [`Command`]s
//! into wire payloads.
//!
//! # The Session Lifecycle
//!
//! 1. **Initialization**: Create a new session with [`ProtocolSession::new`].
//!    At this stage, the session is waiting for the device to introduce itself.
//! 2. **Full Sync**: When first connected (or after a handshake), the device sends
//!    a full state snapshot (`fullSync`). Ingesting this yields [`SessionUpdate::Ready`],
//!    which populates [`DeviceCapabilities`] and yields `initial_events` describing
//!    the current state of all faders, mutes, and routing.
//! 3. **Steady State**: While connected, incoming messages yield [`SessionUpdate::Event`]
//!    for real-time updates (fader movements, knob adjustments, button presses).
//!    You can also call [`ProtocolSession::encode`] to send commands to the device.
//! 4. **Resync**: If the device tree topology changes (e.g. major mode switch),
//!    the session yields [`SessionUpdate::NeedsFullSync`] and resets its layout.
//!    Your transport should request a fresh full sync before resuming commands.

use crate::change_frame::{decode, ChangeFrame};
use crate::commands::EncodeError;
use crate::events::{decode_event_from_frame, extract_initial_state};
use crate::layout::BuildError;
use crate::{Command, DeviceCapabilities, DeviceEvent, Layout};

/// Result of ingesting one complete JUCE change-frame payload.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionUpdate {
    /// A full sync established or replaced the device layout.
    Ready {
        /// Semantic state extracted from the complete full sync.
        initial_events: Vec<DeviceEvent>,
    },
    /// One incremental device event.
    Event(DeviceEvent),
    /// Tree topology changed. Request a fresh full sync before sending commands
    /// or interpreting further property paths.
    NeedsFullSync,
}

/// Failure to decode a payload, establish a layout, or encode a command.
#[derive(Debug)]
pub enum SessionError {
    /// The payload is not exactly one valid JUCE change frame.
    Decode,
    /// A full sync did not contain the required device topology.
    Layout(BuildError),
    /// A command or incremental event was attempted before a valid full sync.
    NotReady,
    /// The command is not valid for the discovered layout.
    Encode(EncodeError),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Decode => f.write_str("invalid or incomplete protocol payload"),
            SessionError::Layout(error) => write!(f, "could not build device layout: {error}"),
            SessionError::NotReady => f.write_str("protocol session needs a full sync"),
            SessionError::Encode(error) => write!(f, "could not encode command: {error}"),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SessionError::Layout(error) => Some(error),
            SessionError::Encode(error) => Some(error),
            SessionError::Decode | SessionError::NotReady => None,
        }
    }
}

/// Stateful, transport-independent RODECaster protocol session.
#[derive(Debug, Default)]
pub struct ProtocolSession {
    layout: Option<Layout>,
    capabilities: Option<DeviceCapabilities>,
}

impl ProtocolSession {
    /// Create a session waiting for its first full sync.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a valid full sync has established the current address layout.
    pub fn is_ready(&self) -> bool {
        self.layout.is_some()
    }

    /// Capabilities discovered from the most recent full sync.
    pub fn capabilities(&self) -> Option<&DeviceCapabilities> {
        self.capabilities.as_ref()
    }

    /// Current low-level address layout for advanced consumers.
    pub fn layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    /// Forget the current layout and return to the pre-full-sync state.
    pub fn reset(&mut self) {
        self.layout = None;
        self.capabilities = None;
    }

    /// Ingest exactly one change-frame payload, without a TCP or USB wrapper.
    pub fn ingest(&mut self, payload: &[u8]) -> Result<SessionUpdate, SessionError> {
        let frame = decode(payload).ok_or(SessionError::Decode)?;
        match frame {
            ChangeFrame::FullSync { root } => {
                // A replacement sync supersedes the old address space even if
                // the new topology is unsupported. Never keep encoding through
                // a stale layout after observing a failed resync.
                self.reset();
                let layout = Layout::from_full_sync(&root).map_err(SessionError::Layout)?;
                let capabilities = DeviceCapabilities::discover(&root, &layout);
                let initial_events = extract_initial_state(&root, &layout);
                self.layout = Some(layout);
                self.capabilities = Some(capabilities);
                Ok(SessionUpdate::Ready { initial_events })
            }
            ChangeFrame::ChildAdded { .. }
            | ChangeFrame::ChildRemoved { .. }
            | ChangeFrame::ChildMoved { .. } => {
                self.reset();
                Ok(SessionUpdate::NeedsFullSync)
            }
            ChangeFrame::PropertyChanged { .. } | ChangeFrame::PropertyRemoved { .. } => {
                let layout = self.layout.as_ref().ok_or(SessionError::NotReady)?;
                let event = decode_event_from_frame(frame, layout);
                Ok(SessionUpdate::Event(event))
            }
        }
    }

    /// Encode a typed command using the layout from the most recent full sync.
    pub fn encode(&self, command: &Command) -> Result<Vec<Vec<u8>>, SessionError> {
        let layout = self.layout.as_ref().ok_or(SessionError::NotReady)?;
        command.encode(layout).map_err(SessionError::Encode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_frame::encode_property_changed;
    use crate::Value;

    #[test]
    fn rejects_incremental_event_before_full_sync() {
        let mut session = ProtocolSession::new();
        let payload = encode_property_changed(&[], "property", &Value::Bool(true));
        assert!(matches!(
            session.ingest(&payload),
            Err(SessionError::NotReady)
        ));
    }

    #[test]
    fn structural_change_invalidates_session() {
        let mut session = ProtocolSession::new();
        // The lifecycle rule does not require an existing layout: either way a
        // structural change means the next useful message must be a full sync.
        let child_moved = [0x05, 0x00, 0x01, 0x00, 0x01, 0x01];
        assert_eq!(
            session.ingest(&child_moved).unwrap(),
            SessionUpdate::NeedsFullSync
        );
        assert!(!session.is_ready());
    }
}
