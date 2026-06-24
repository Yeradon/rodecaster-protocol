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

use std::fmt;
use std::str::FromStr;

/// Which RODECaster a fullSync came from. Detected from the `SYSTEM` node;
/// selects the per-model [`Fader`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceModel {
    /// RODECaster Pro II: 6 physical + 3 virtual faders.
    Pro2,
    /// RODECaster Duo: 4 physical + 5 virtual faders.
    Duo,
}

impl DeviceModel {
    /// Map the `SYSTEM.boardType` integer (`0` = Pro II, `1` = Duo). Returns
    /// `None` for an unrecognized board so [`DeviceModel::detect`] can fall back
    /// to the name.
    pub fn from_board_type(board_type: i64) -> Option<Self> {
        match board_type {
            0 => Some(Self::Pro2),
            1 => Some(Self::Duo),
            _ => None,
        }
    }

    /// Map the `SYSTEM.systemName` string: contains "duo" (case-insensitive) =>
    /// Duo, else Pro II. Firmware 1.6.8 omits this property; newer firmware
    /// includes it, which is why [`DeviceModel::detect`] prefers `boardType`.
    pub fn from_system_name(system_name: &str) -> Self {
        if system_name.to_lowercase().contains("duo") {
            Self::Duo
        } else {
            Self::Pro2
        }
    }

    /// Detect the model from the `SYSTEM` node's `boardType` and `systemName`.
    /// `boardType` is authoritative (present and correct on every firmware
    /// observed); `systemName` is the fallback; with neither, default to Pro II.
    pub fn detect(board_type: Option<i64>, system_name: Option<&str>) -> Self {
        if let Some(model) = board_type.and_then(Self::from_board_type) {
            return model;
        }
        if let Some(name) = system_name {
            return Self::from_system_name(name);
        }
        Self::Pro2
    }
}

impl fmt::Display for DeviceModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pro2 => "RODECaster Pro II",
            Self::Duo => "RODECaster Duo",
        })
    }
}

/// A mix output bus: where audio is routed *to*. Model-independent (13 buses).
/// The protocol mix index equals enum order (`Headphone1` = 0 .. `CallMe3` =
/// 12); it is the second dimension of the source-major mix matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixOutput {
    Headphone1,
    Headphone2,
    Headphone3,
    Headphone4,
    Speaker,
    Recording,
    Bluetooth,
    Usb1,
    Chat,
    Usb2,
    CallMe1,
    CallMe2,
    CallMe3,
}

impl MixOutput {
    /// The protocol mix index (0..=12).
    pub fn to_protocol(self) -> u8 {
        match self {
            Self::Headphone1 => 0,
            Self::Headphone2 => 1,
            Self::Headphone3 => 2,
            Self::Headphone4 => 3,
            Self::Speaker => 4,
            Self::Recording => 5,
            Self::Bluetooth => 6,
            Self::Usb1 => 7,
            Self::Chat => 8,
            Self::Usb2 => 9,
            Self::CallMe1 => 10,
            Self::CallMe2 => 11,
            Self::CallMe3 => 12,
        }
    }

    /// Inverse of [`MixOutput::to_protocol`]; `None` for an out-of-range index.
    pub fn from_protocol(idx: u8) -> Option<Self> {
        Some(match idx {
            0 => Self::Headphone1,
            1 => Self::Headphone2,
            2 => Self::Headphone3,
            3 => Self::Headphone4,
            4 => Self::Speaker,
            5 => Self::Recording,
            6 => Self::Bluetooth,
            7 => Self::Usb1,
            8 => Self::Chat,
            9 => Self::Usb2,
            10 => Self::CallMe1,
            11 => Self::CallMe2,
            12 => Self::CallMe3,
            _ => return None,
        })
    }

    /// Human-readable label for UIs and logs.
    pub fn label(self) -> &'static str {
        match self {
            Self::Headphone1 => "Headphone 1",
            Self::Headphone2 => "Headphone 2",
            Self::Headphone3 => "Headphone 3",
            Self::Headphone4 => "Headphone 4",
            Self::Speaker => "Speaker",
            Self::Recording => "Recording",
            Self::Bluetooth => "Bluetooth",
            Self::Usb1 => "USB 1",
            Self::Chat => "Chat",
            Self::Usb2 => "USB 2",
            Self::CallMe1 => "CallMe 1",
            Self::CallMe2 => "CallMe 2",
            Self::CallMe3 => "CallMe 3",
        }
    }
}

impl FromStr for MixOutput {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "headphone1" | "hp1" => Ok(Self::Headphone1),
            "headphone2" | "hp2" => Ok(Self::Headphone2),
            "headphone3" | "hp3" => Ok(Self::Headphone3),
            "headphone4" | "hp4" => Ok(Self::Headphone4),
            "speaker" | "spk" | "monitor" => Ok(Self::Speaker),
            "recording" | "rec" => Ok(Self::Recording),
            "bluetooth" | "bt" => Ok(Self::Bluetooth),
            "usb1" => Ok(Self::Usb1),
            "chat" => Ok(Self::Chat),
            "usb2" => Ok(Self::Usb2),
            "callme1" | "cm1" => Ok(Self::CallMe1),
            "callme2" | "cm2" => Ok(Self::CallMe2),
            "callme3" | "cm3" => Ok(Self::CallMe3),
            _ => Err(format!(
                "unknown mix output: {s} (try: hp1, speaker, bt, cm1)"
            )),
        }
    }
}

impl fmt::Display for MixOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Headphone1 => "headphone1",
            Self::Headphone2 => "headphone2",
            Self::Headphone3 => "headphone3",
            Self::Headphone4 => "headphone4",
            Self::Speaker => "speaker",
            Self::Recording => "recording",
            Self::Bluetooth => "bluetooth",
            Self::Usb1 => "usb1",
            Self::Chat => "chat",
            Self::Usb2 => "usb2",
            Self::CallMe1 => "callme1",
            Self::CallMe2 => "callme2",
            Self::CallMe3 => "callme3",
        })
    }
}

/// An audio source: where audio comes *from*. Model-independent (19 sources,
/// one shared device picker across the Pro family). The protocol source index
/// equals enum order (`Combo1` = 0 .. `CallMe3` = 18); it is the first
/// dimension (source-major) of the mix matrix.
///
/// `CallMe1..=CallMe3` (16..=18) are return channels addressed outside the
/// regular matrix on some firmware; see [`Source::is_callme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Combo1,
    Combo2,
    Combo3,
    Combo4,
    Combo1_2,
    Combo2_3,
    Combo3_4,
    Usb1,
    Chat,
    Usb2,
    Bluetooth,
    SoundPad,
    VirtualGame,
    VirtualMusic,
    VirtualA,
    VirtualB,
    CallMe1,
    CallMe2,
    CallMe3,
}

impl Source {
    /// The protocol source index (0..=18).
    pub fn to_protocol(self) -> u8 {
        match self {
            Self::Combo1 => 0,
            Self::Combo2 => 1,
            Self::Combo3 => 2,
            Self::Combo4 => 3,
            Self::Combo1_2 => 4,
            Self::Combo2_3 => 5,
            Self::Combo3_4 => 6,
            Self::Usb1 => 7,
            Self::Chat => 8,
            Self::Usb2 => 9,
            Self::Bluetooth => 10,
            Self::SoundPad => 11,
            Self::VirtualGame => 12,
            Self::VirtualMusic => 13,
            Self::VirtualA => 14,
            Self::VirtualB => 15,
            Self::CallMe1 => 16,
            Self::CallMe2 => 17,
            Self::CallMe3 => 18,
        }
    }

    /// Inverse of [`Source::to_protocol`]; `None` for an out-of-range index.
    pub fn from_protocol(idx: u8) -> Option<Self> {
        Some(match idx {
            0 => Self::Combo1,
            1 => Self::Combo2,
            2 => Self::Combo3,
            3 => Self::Combo4,
            4 => Self::Combo1_2,
            5 => Self::Combo2_3,
            6 => Self::Combo3_4,
            7 => Self::Usb1,
            8 => Self::Chat,
            9 => Self::Usb2,
            10 => Self::Bluetooth,
            11 => Self::SoundPad,
            12 => Self::VirtualGame,
            13 => Self::VirtualMusic,
            14 => Self::VirtualA,
            15 => Self::VirtualB,
            16 => Self::CallMe1,
            17 => Self::CallMe2,
            18 => Self::CallMe3,
            _ => return None,
        })
    }

    /// CallMe return channels are routed through a dedicated request path
    /// outside the regular mix matrix on firmware where they sit past
    /// `Layout::source_count`. Encoders use this to pick the wire form.
    pub fn is_callme(self) -> bool {
        matches!(self, Self::CallMe1 | Self::CallMe2 | Self::CallMe3)
    }

    /// Human-readable label for UIs and logs.
    pub fn label(self) -> &'static str {
        match self {
            Self::Combo1 => "Combo 1",
            Self::Combo2 => "Combo 2",
            Self::Combo3 => "Combo 3",
            Self::Combo4 => "Combo 4",
            Self::Combo1_2 => "Combo 1+2",
            Self::Combo2_3 => "Combo 2+3",
            Self::Combo3_4 => "Combo 3+4",
            Self::Usb1 => "USB 1",
            Self::Chat => "Chat",
            Self::Usb2 => "USB 2",
            Self::Bluetooth => "Bluetooth",
            Self::SoundPad => "Sound Pad",
            Self::VirtualGame => "Game",
            Self::VirtualMusic => "Music",
            Self::VirtualA => "Virtual A",
            Self::VirtualB => "Virtual B",
            Self::CallMe1 => "CallMe 1",
            Self::CallMe2 => "CallMe 2",
            Self::CallMe3 => "CallMe 3",
        }
    }
}

impl FromStr for Source {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "combo1" | "mic1" => Ok(Self::Combo1),
            "combo2" | "mic2" => Ok(Self::Combo2),
            "combo3" | "mic3" => Ok(Self::Combo3),
            "combo4" | "mic4" => Ok(Self::Combo4),
            "combo1_2" | "combo12" => Ok(Self::Combo1_2),
            "combo2_3" | "combo23" => Ok(Self::Combo2_3),
            "combo3_4" | "combo34" => Ok(Self::Combo3_4),
            "usb1" => Ok(Self::Usb1),
            "chat" => Ok(Self::Chat),
            "usb2" => Ok(Self::Usb2),
            "bluetooth" | "bt" => Ok(Self::Bluetooth),
            "soundpad" | "pad" => Ok(Self::SoundPad),
            "virtualgame" | "game" | "vgame" => Ok(Self::VirtualGame),
            "virtualmusic" | "music" | "vmusic" => Ok(Self::VirtualMusic),
            "virtuala" | "va" => Ok(Self::VirtualA),
            "virtualb" | "vb" => Ok(Self::VirtualB),
            "callme1" | "cm1" => Ok(Self::CallMe1),
            "callme2" | "cm2" => Ok(Self::CallMe2),
            "callme3" | "cm3" => Ok(Self::CallMe3),
            _ => Err(format!("unknown source: {s} (try: combo1, bt, game, cm1)")),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Combo1 => "combo1",
            Self::Combo2 => "combo2",
            Self::Combo3 => "combo3",
            Self::Combo4 => "combo4",
            Self::Combo1_2 => "combo1_2",
            Self::Combo2_3 => "combo2_3",
            Self::Combo3_4 => "combo3_4",
            Self::Usb1 => "usb1",
            Self::Chat => "chat",
            Self::Usb2 => "usb2",
            Self::Bluetooth => "bluetooth",
            Self::SoundPad => "soundpad",
            Self::VirtualGame => "game",
            Self::VirtualMusic => "music",
            Self::VirtualA => "virtuala",
            Self::VirtualB => "virtualb",
            Self::CallMe1 => "callme1",
            Self::CallMe2 => "callme2",
            Self::CallMe3 => "callme3",
        })
    }
}

/// A fader strip: a physical or virtual channel the user mixes. The strip <->
/// wire-index map is **model-dependent**, so [`Fader::to_index`] and
/// [`Fader::from_index`] both take a [`DeviceModel`]:
///
/// - Pro II: `Physical1..=Physical6` => 0..=5, `Virtual1..=Virtual3` => 6..=8.
/// - Duo: `Physical1..=Physical4` => 0..=3, `Virtual1..=Virtual5` => 4..=8.
///
/// A variant absent on a model (e.g. `Physical6` on the Duo) has no index
/// there; the conversions return `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fader {
    Physical1,
    Physical2,
    Physical3,
    Physical4,
    Physical5,
    Physical6,
    Virtual1,
    Virtual2,
    Virtual3,
    Virtual4,
    Virtual5,
}

impl Fader {
    /// The wire fader/channel index for this strip on `model`, or `None` if the
    /// strip does not exist on that model.
    pub fn to_index(self, model: DeviceModel) -> Option<u8> {
        use DeviceModel::{Duo, Pro2};
        use Fader::*;
        let idx = match (model, self) {
            (Pro2, Physical1) => 0,
            (Pro2, Physical2) => 1,
            (Pro2, Physical3) => 2,
            (Pro2, Physical4) => 3,
            (Pro2, Physical5) => 4,
            (Pro2, Physical6) => 5,
            (Pro2, Virtual1) => 6,
            (Pro2, Virtual2) => 7,
            (Pro2, Virtual3) => 8,
            (Pro2, Virtual4 | Virtual5) => return None,

            (Duo, Physical1) => 0,
            (Duo, Physical2) => 1,
            (Duo, Physical3) => 2,
            (Duo, Physical4) => 3,
            (Duo, Physical5 | Physical6) => return None,
            (Duo, Virtual1) => 4,
            (Duo, Virtual2) => 5,
            (Duo, Virtual3) => 6,
            (Duo, Virtual4) => 7,
            (Duo, Virtual5) => 8,
        };
        Some(idx)
    }

    /// Inverse of [`Fader::to_index`]: the strip at wire index `idx` on `model`,
    /// or `None` if no strip maps there (e.g. index 9, the master channel).
    pub fn from_index(model: DeviceModel, idx: u8) -> Option<Self> {
        use DeviceModel::{Duo, Pro2};
        use Fader::*;
        Some(match (model, idx) {
            (Pro2, 0) => Physical1,
            (Pro2, 1) => Physical2,
            (Pro2, 2) => Physical3,
            (Pro2, 3) => Physical4,
            (Pro2, 4) => Physical5,
            (Pro2, 5) => Physical6,
            (Pro2, 6) => Virtual1,
            (Pro2, 7) => Virtual2,
            (Pro2, 8) => Virtual3,

            (Duo, 0) => Physical1,
            (Duo, 1) => Physical2,
            (Duo, 2) => Physical3,
            (Duo, 3) => Physical4,
            (Duo, 4) => Virtual1,
            (Duo, 5) => Virtual2,
            (Duo, 6) => Virtual3,
            (Duo, 7) => Virtual4,
            (Duo, 8) => Virtual5,

            _ => return None,
        })
    }

    /// `true` if this strip is a hardware fader on `model` (driven by the motor
    /// fader over MIDI/UART), as opposed to a virtual strip set in software.
    pub fn is_physical(self, model: DeviceModel) -> bool {
        use DeviceModel::{Duo, Pro2};
        use Fader::*;
        match model {
            Pro2 => matches!(
                self,
                Physical1 | Physical2 | Physical3 | Physical4 | Physical5 | Physical6
            ),
            Duo => matches!(self, Physical1 | Physical2 | Physical3 | Physical4),
        }
    }

    /// Human-readable label for UIs and logs.
    pub fn label(self) -> &'static str {
        match self {
            Self::Physical1 => "Fader 1",
            Self::Physical2 => "Fader 2",
            Self::Physical3 => "Fader 3",
            Self::Physical4 => "Fader 4",
            Self::Physical5 => "Fader 5",
            Self::Physical6 => "Fader 6",
            Self::Virtual1 => "Virtual 1",
            Self::Virtual2 => "Virtual 2",
            Self::Virtual3 => "Virtual 3",
            Self::Virtual4 => "Virtual 4",
            Self::Virtual5 => "Virtual 5",
        }
    }
}

impl FromStr for Fader {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "physical1" | "p1" | "fader1" => Ok(Self::Physical1),
            "physical2" | "p2" | "fader2" => Ok(Self::Physical2),
            "physical3" | "p3" | "fader3" => Ok(Self::Physical3),
            "physical4" | "p4" | "fader4" => Ok(Self::Physical4),
            "physical5" | "p5" | "fader5" => Ok(Self::Physical5),
            "physical6" | "p6" | "fader6" => Ok(Self::Physical6),
            "virtual1" | "v1" | "vfader1" => Ok(Self::Virtual1),
            "virtual2" | "v2" | "vfader2" => Ok(Self::Virtual2),
            "virtual3" | "v3" | "vfader3" => Ok(Self::Virtual3),
            "virtual4" | "v4" | "vfader4" => Ok(Self::Virtual4),
            "virtual5" | "v5" | "vfader5" => Ok(Self::Virtual5),
            _ => Err(format!("unknown fader: {s} (try: p1, fader1, v1)")),
        }
    }
}

impl fmt::Display for Fader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Physical1 => "physical1",
            Self::Physical2 => "physical2",
            Self::Physical3 => "physical3",
            Self::Physical4 => "physical4",
            Self::Physical5 => "physical5",
            Self::Physical6 => "physical6",
            Self::Virtual1 => "virtual1",
            Self::Virtual2 => "virtual2",
            Self::Virtual3 => "virtual3",
            Self::Virtual4 => "virtual4",
            Self::Virtual5 => "virtual5",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_model_prefers_board_type_over_name() {
        // boardType wins even if the name disagrees.
        assert_eq!(
            DeviceModel::detect(Some(1), Some("RODECaster Pro II")),
            DeviceModel::Duo
        );
        assert_eq!(DeviceModel::detect(Some(0), None), DeviceModel::Pro2);
    }

    #[test]
    fn device_model_falls_back_to_name_then_default() {
        // Unrecognized board -> use the name.
        assert_eq!(
            DeviceModel::detect(Some(99), Some("rodecaster duo")),
            DeviceModel::Duo
        );
        // No board, no name -> default Pro II (matches fw 1.6.8 captures).
        assert_eq!(DeviceModel::detect(None, None), DeviceModel::Pro2);
        // Name only.
        assert_eq!(
            DeviceModel::detect(None, Some("Some Duo")),
            DeviceModel::Duo
        );
        assert_eq!(DeviceModel::detect(None, Some("Pro II")), DeviceModel::Pro2);
    }

    #[test]
    fn source_protocol_roundtrips_all_19() {
        for i in 0..=18u8 {
            let s = Source::from_protocol(i).expect("valid source index");
            assert_eq!(s.to_protocol(), i);
        }
        assert_eq!(Source::from_protocol(19), None);
    }

    #[test]
    fn mix_output_protocol_roundtrips_all_13() {
        for i in 0..=12u8 {
            let m = MixOutput::from_protocol(i).expect("valid mix index");
            assert_eq!(m.to_protocol(), i);
        }
        assert_eq!(MixOutput::from_protocol(13), None);
    }

    #[test]
    fn source_callme_classification() {
        assert!(Source::CallMe1.is_callme());
        assert!(Source::CallMe3.is_callme());
        assert!(!Source::Combo1.is_callme());
        assert!(!Source::VirtualB.is_callme());
    }

    #[test]
    fn fader_index_roundtrips_per_model() {
        // Every wire index 0..=8 maps to exactly one strip on each model and back.
        for model in [DeviceModel::Pro2, DeviceModel::Duo] {
            for idx in 0..=8u8 {
                let f = Fader::from_index(model, idx).expect("strip at index");
                assert_eq!(f.to_index(model), Some(idx), "{model:?} idx {idx}");
            }
            // Index 9 (master channel) has no fader on either model.
            assert_eq!(Fader::from_index(model, 9), None);
        }
    }

    #[test]
    fn fader_layout_differs_by_model() {
        // Pro II: 6 physical then 3 virtual.
        assert_eq!(Fader::Physical6.to_index(DeviceModel::Pro2), Some(5));
        assert_eq!(Fader::Virtual1.to_index(DeviceModel::Pro2), Some(6));
        assert_eq!(Fader::Virtual3.to_index(DeviceModel::Pro2), Some(8));
        // Pro II has no Virtual4/5.
        assert_eq!(Fader::Virtual4.to_index(DeviceModel::Pro2), None);
        assert_eq!(Fader::Virtual5.to_index(DeviceModel::Pro2), None);

        // Duo: 4 physical then 5 virtual.
        assert_eq!(Fader::Physical4.to_index(DeviceModel::Duo), Some(3));
        assert_eq!(Fader::Virtual1.to_index(DeviceModel::Duo), Some(4));
        assert_eq!(Fader::Virtual5.to_index(DeviceModel::Duo), Some(8));
        // Duo has no Physical5/6.
        assert_eq!(Fader::Physical5.to_index(DeviceModel::Duo), None);
        assert_eq!(Fader::Physical6.to_index(DeviceModel::Duo), None);
    }

    #[test]
    fn fader_is_physical_per_model() {
        assert!(Fader::Physical6.is_physical(DeviceModel::Pro2));
        assert!(!Fader::Virtual1.is_physical(DeviceModel::Pro2));
        assert!(Fader::Physical4.is_physical(DeviceModel::Duo));
        // Physical5/6 don't exist on the Duo, so they aren't hardware faders there.
        assert!(!Fader::Physical5.is_physical(DeviceModel::Duo));
    }

    #[test]
    fn display_roundtrips_through_fromstr() {
        // Each canonical Display string parses back to the same variant.
        let sources = [
            Source::Combo1,
            Source::Combo3_4,
            Source::Bluetooth,
            Source::VirtualGame,
            Source::CallMe3,
        ];
        for s in sources {
            assert_eq!(s.to_string().parse::<Source>(), Ok(s));
        }
        let mixes = [
            MixOutput::Headphone1,
            MixOutput::Speaker,
            MixOutput::CallMe2,
        ];
        for m in mixes {
            assert_eq!(m.to_string().parse::<MixOutput>(), Ok(m));
        }
        let faders = [
            Fader::Physical1,
            Fader::Physical6,
            Fader::Virtual1,
            Fader::Virtual5,
        ];
        for fd in faders {
            assert_eq!(fd.to_string().parse::<Fader>(), Ok(fd));
        }
    }

    #[test]
    fn fromstr_aliases() {
        assert_eq!("mic1".parse::<Source>(), Ok(Source::Combo1));
        assert_eq!("combo12".parse::<Source>(), Ok(Source::Combo1_2));
        assert_eq!("bt".parse::<Source>(), Ok(Source::Bluetooth));
        assert_eq!("hp3".parse::<MixOutput>(), Ok(MixOutput::Headphone3));
        assert_eq!("spk".parse::<MixOutput>(), Ok(MixOutput::Speaker));
        assert_eq!("p1".parse::<Fader>(), Ok(Fader::Physical1));
        assert_eq!("fader4".parse::<Fader>(), Ok(Fader::Physical4));
        assert_eq!("v5".parse::<Fader>(), Ok(Fader::Virtual5));
        assert!("nonsense".parse::<Source>().is_err());
    }
}
