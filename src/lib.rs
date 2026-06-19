//! JUCE `var` / `ValueTree` codec and transport frame for the RODECaster
//! Pro II / Duo wire protocol.
//!
//! The device speaks `juce::ValueTreeSynchroniser` over a length-prefixed
//! transport. This crate owns the byte formats once, for both directions:
//!
//! - [`juce_var`]: the per-value `juce::var` codec ([`Value`], [`read_value`]).
//! - [`valuetree`]: the full-sync tree decode ([`parse_valuetree`], [`Node`]).
//! - [`frame`]: the length-prefixed transport [`Packet`].
//! - [`command`]: the [`RodeCommand`] encode trait.

pub mod command;
pub mod frame;
pub mod juce_var;
pub mod valuetree;

pub use command::RodeCommand;
pub use frame::{Packet, MAGIC_HEADER};
pub use juce_var::{read_value, Reader, Value};
pub use valuetree::{parse_valuetree, Node, Property};
