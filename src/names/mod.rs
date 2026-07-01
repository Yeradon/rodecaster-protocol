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

mod app;
mod audio;
mod build;
mod channel;
mod core;
mod ducker;
mod effects;
mod fx_preset;
mod gui;
mod headphone;
mod input_source;
mod master;
mod meter;
mod mix_minuses;
mod network;
mod output;
mod pad;
mod pad_recorder;
mod player;
mod radio;
mod rcsync;
mod recorder;
mod recording;
mod show;
mod sip;
mod storage;
mod streamerx;
mod system;
mod test;
mod theme;
mod wifi_scan_result;

pub use app::AppParam;
pub use audio::AudioParam;
pub use build::BuildParam;
pub use channel::ChannelParam;
pub use core::{DeviceModel, Fader, MixOutput, Source};
pub use ducker::DuckerParam;
pub use effects::EffectsParam;
pub use fx_preset::FxPresetParam;
pub use gui::GuiParam;
pub use headphone::HeadphoneParam;
pub use input_source::InputSourceParam;
pub use master::MasterParam;
pub use meter::MeterParam;
pub use mix_minuses::MixMinusesParam;
pub use network::NetworkParam;
pub use output::OutputParam;
pub use pad::PadParam;
pub use pad_recorder::PadRecorderParam;
pub use player::PlayerParam;
pub use radio::{RadioParam, RadioRxParam, RadioTxParam};
pub use rcsync::RcSyncMixParam;
pub use recorder::RecorderParam;
pub use recording::{RecordingParam, RecordingsParam};
pub use show::{CurrentShowParam, ShowControlParam, ShowParam};
pub use sip::{SipAdvancedParam, SipCallSlotsParam, SipCallingParam, SipRegistrationParam};
pub use storage::StorageVolumeParam;
pub use streamerx::{StreamerXMixPresetParam, StreamerXStreamMixParam};
pub use system::SystemParam;
pub use test::TestParam;
pub use theme::ThemeParam;
pub use wifi_scan_result::WifiScanResultParam;
