//! Device capability discovery and hardware feature inspection.
//!
//! [`DeviceCapabilities`] describes the controls, faders, inputs, outputs, and
//! audio features supported by the connected console.
//!
//! The RØDECaster Duo (4 physical faders) and Pro II (6 physical faders) share
//! the wire protocol but differ in physical surfaces and channel counts.
//! Inspecting [`DeviceCapabilities`] allows applications to adapt to either console
//! dynamically rather than hardcoding model-specific assumptions.
/// Addressable controls and family counts exposed by one connected device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCapabilities {
    model: DeviceModel,
    firmware: Option<String>,
    faders: Vec<Fader>,
    sources: Vec<Source>,
    mix_outputs: Vec<MixOutput>,
    channel_count: u8,
    input_source_count: u8,
    headphone_count: u8,
    effects_count: u8,
    pad_count: u8,
    sip_call_slots_count: u8,
    sip_registration_count: u8,
}

impl DeviceCapabilities {
    pub(crate) fn discover(root: &Node, layout: &Layout) -> Self {
        let model = layout.model();
        Self {
            model,
            firmware: find_string_property(root, "systemFirmwareVersion").map(str::to_owned),
            faders: (0..layout.fader_count())
                .filter_map(|index| Fader::from_index(model, index))
                .collect(),
            sources: (0..layout.source_count())
                .filter_map(Source::from_protocol)
                .collect(),
            mix_outputs: (0..layout.mix_count_per_source())
                .filter_map(MixOutput::from_protocol)
                .collect(),
            channel_count: layout.channel_count(),
            input_source_count: layout.input_source_count(),
            headphone_count: layout.headphone_count(),
            effects_count: layout.effects_count(),
            pad_count: layout.pad_count(),
            sip_call_slots_count: layout.sip_call_slots_count(),
            sip_registration_count: layout.sip_registration_count(),
        }
    }

    /// Detected hardware model.
    pub fn model(&self) -> DeviceModel {
        self.model
    }

    /// Firmware version reported by the full sync, when present.
    pub fn firmware(&self) -> Option<&str> {
        self.firmware.as_deref()
    }

    /// Addressable fader strips in protocol order.
    pub fn faders(&self) -> &[Fader] {
        &self.faders
    }

    /// Addressable input sources in protocol order.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    /// Addressable mix outputs in protocol order.
    pub fn mix_outputs(&self) -> &[MixOutput] {
        &self.mix_outputs
    }

    /// Whether this exact device topology contains the named fader.
    pub fn supports_fader(&self, fader: Fader) -> bool {
        self.faders.contains(&fader)
    }

    pub fn channel_count(&self) -> u8 {
        self.channel_count
    }

    pub fn input_source_count(&self) -> u8 {
        self.input_source_count
    }

    pub fn headphone_count(&self) -> u8 {
        self.headphone_count
    }

    pub fn effects_count(&self) -> u8 {
        self.effects_count
    }

    pub fn pad_count(&self) -> u8 {
        self.pad_count
    }

    pub fn sip_call_slots_count(&self) -> u8 {
        self.sip_call_slots_count
    }

    pub fn sip_registration_count(&self) -> u8 {
        self.sip_registration_count
    }
}

fn find_string_property<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.properties
        .iter()
        .find(|property| property.name == name)
        .and_then(|property| match &property.value {
            Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .or_else(|| {
            node.children
                .iter()
                .find_map(|child| find_string_property(child, name))
        })
}
