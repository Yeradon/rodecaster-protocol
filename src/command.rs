//! The command-encoding interface.

/// A command that can be encoded into a RODECaster protocol payload.
///
/// Implementors build the inner `juce::var` payload (see [`crate::juce_var`]);
/// wrap the result in a [`crate::frame::Packet`] for the transport.
pub trait RodeCommand {
    fn build_payload(&self, session_id: &[u8]) -> Vec<u8>;
}
