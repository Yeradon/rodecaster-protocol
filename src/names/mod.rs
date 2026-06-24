//! Named Rodecaster entities: the typed vocabulary the public API speaks.
//!
//! ## Why names, not indices
//!
//! On the wire every entity is purely positional: a source is "the Nth
//! `INPUTSOURCE` node", a mix cell is "source-major cell N", a fader is "the
//! Nth `FADER` node". Those ordinals are an implementation detail of the JUCE
//! tree, not a stable contract: they shift with firmware and differ across
//! device models. This module pins each ordinal to a *named* entity
//! ([`Source`], [`MixOutput`], [`Fader`]) so consumers address "Combo 1" or
//! "Virtual 2", never a bare `7`.
//!
//! The name <-> ordinal mapping is irreducible reverse-engineering knowledge:
//! the nodes carry no labels, so it cannot be recovered from a fullSync. It
//! lives here as data.
//!
//! ## Per-model split
//!
//! [`Source`] (19) and [`MixOutput`] (13) are model-independent: the RODECaster
//! Pro family shares one source/output vocabulary (one shared device picker).
//! Only [`Fader`] differs: the Pro II exposes 6 physical + 3 virtual strips,
//! the Duo 4 physical + 5 virtual. So the fader name <-> index map is
//! parameterized by [`DeviceModel`], which [`crate::Layout`] detects from the
//! fullSync's `SYSTEM` node.
//!
//! ## Module layout
//!
//! [`macros`] defines the one `wire_param_enum!` macro; [`core`] holds the
//! model-independent vocabulary ([`DeviceModel`], [`MixOutput`], [`Source`],
//! [`Fader`]); and each node-scoped property family lives in its own file
//! (`channel`, `input_source`, `master`, ...). Every type is re-exported here so
//! the public path stays `crate::names::X`.

#[macro_use]
mod macros;

mod channel;
mod core;
mod ducker;
mod effects;
mod gui;
mod headphone;
mod input_source;
mod master;
mod output;
mod pad;
mod player;
mod recorder;

pub use channel::ChannelParam;
pub use core::{DeviceModel, Fader, MixOutput, Source};
pub use ducker::DuckerParam;
pub use effects::EffectsParam;
pub use gui::GuiParam;
pub use headphone::HeadphoneParam;
pub use input_source::InputSourceParam;
pub use master::MasterParam;
pub use output::OutputParam;
pub use pad::PadParam;
pub use player::PlayerParam;
pub use recorder::RecorderParam;
