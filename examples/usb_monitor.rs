//! Minimal high-level USB event monitor.
//!
//! Usage: `cargo run --example usb_monitor -- /dev/hidraw3`

use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};

use rodecaster_protocol::{usb, FrameScan, ProtocolSession, SessionUpdate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or("usage: usb_monitor <path-to-hidraw>")?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;

    // The device stays silent until a client requests its initial full sync.
    (&file).write_all(&usb::Packet::new(usb::HANDSHAKE_BODY.to_vec()).to_bytes())?;

    let mut session = ProtocolSession::new();
    let mut buffered = Vec::new();
    let mut report = [0_u8; usb::REPORT_SIZE];
    loop {
        let read = (&file).read(&mut report)?;
        buffered.extend_from_slice(&report[..read]);

        loop {
            let len = match usb::scan_frame(&buffered) {
                FrameScan::Complete { len } => len,
                FrameScan::Incomplete => break,
                FrameScan::Desync => {
                    buffered.drain(..usb::REPORT_SIZE.min(buffered.len()));
                    continue;
                }
            };
            let (packet, consumed) = usb::Packet::from_bytes(&buffered[..len])
                .ok_or("USB scanner returned an invalid packet")?;
            buffered.drain(..consumed);

            match session.ingest(&packet.payload)? {
                SessionUpdate::Ready { initial_events } => {
                    let capabilities = session.capabilities().unwrap();
                    println!(
                        "connected: {} fw {} ({} faders, {} sources, {} mixes)",
                        capabilities.model(),
                        capabilities.firmware().unwrap_or("unknown"),
                        capabilities.faders().len(),
                        capabilities.sources().len(),
                        capabilities.mix_outputs().len(),
                    );
                    println!("initial state: {} typed events", initial_events.len());
                }
                SessionUpdate::Event(event) => println!("{event:?}"),
                SessionUpdate::NeedsFullSync => {
                    eprintln!("layout changed, requesting a fresh full sync");
                    (&file)
                        .write_all(&usb::Packet::new(usb::HANDSHAKE_BODY.to_vec()).to_bytes())?;
                }
            }
        }
    }
}
