# rodecaster-protocol

A pure-Rust protocol library for the RØDECaster Pro II and RØDECaster Duo.

This crate provides JUCE `ValueTreeSynchroniser` and `var` codecs for parsing
and serializing wire messages used by RØDECaster audio consoles.

It is self-contained, dependency-free (`std` only), and transport-agnostic.

## Features

- **JUCE Codecs**: Codecs for JUCE's binary `var` serialization and `ValueTreeSynchroniser` streams.
- **Transport Framing**: TCP packet framing with magic header (`0xF2B49E2C`) and length prefix.
- **Zero Dependencies**: Pure `std` Rust.

## License

MIT
