# rodecaster-protocol

A pure-Rust protocol library for the RØDECaster Pro II and RØDECaster Duo.

This crate provides typed commands, events, and runtime layout discovery over
JUCE's `ValueTreeSynchroniser` binary protocol.

It is self-contained, dependency-free (`std` only), and transport-agnostic.

## Features

- **Typed Commands and Events**: Faders, mutes, solos, and audio routing matrix controls.
- **Dynamic Layout Discovery**: Reads device state tree on connection to determine model and channels at runtime.
- **Transport Codecs**: Byte framing for USB HID reports (`usb`) and TCP streams (`frame`).
- **Zero Dependencies**: Pure `std` Rust.

## License

MIT
