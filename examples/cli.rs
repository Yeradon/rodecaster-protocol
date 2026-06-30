//! Interactive REPL for manual testing of the protocol crate.
//! Transport-agnostic: the same [`Command`]/[`DeviceEvent`] vocabulary drives
//! the device over either USB HID (`/dev/hidraw*` + the crate's [`usb`] frame)
//! or TCP (the crate's [`frame`] format). Only the I/O loop and the
//! outer framing differ; everything from `parse_valuetree` and [`Layout`] on
//! up is reused.
//!
//! ## Run
//!
//! ```text
//! # USB (auto-detect a Pro II / Duo by VID:PID on any /dev/hidrawN):
//! nix develop --command cargo run --example cli
//! nix develop --command cargo run --example cli -- usb
//! nix develop --command cargo run --example cli -- usb /dev/hidraw3
//!
//! # TCP (direct device port or network proxy):
//! nix develop --command cargo run --example cli -- tcp 192.168.1.5:2345
//! nix develop --command cargo run --example cli -- tcp 127.0.0.1:2346
//! ```
//!
//! USB needs read+write on `/dev/hidraw*` (sudo, or a udev rule for VID
//! `19f7`). TCP needs network reach to the target address.
//!
//! ## Commands
//!
//! Type `help` at the prompt for the full list. Quick start:
//!
//! ```text
//! > layout
//! > watch on            # live event print on (default)
//! > mute p1 on          # mute physical fader 1
//! > level v1 64         # set virtual fader 1 to mid scale
//! > link usb1 hp1       # native link USB-1 source -> Headphone-1 mix
//! > set master CompellorOn true   # generic typed-family setter
//! > quit
//! ```
//!
//! Linux-only for USB (uses `/dev/hidraw*` and sysfs); TCP works anywhere.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rodecaster_protocol::{
    change_frame, decode_event, frame, parse_valuetree, usb, AppParam, AudioParam, BuildParam,
    ChannelParam, Command, CurrentShowParam, DeviceEvent, DeviceModel, DuckerParam, EffectsParam,
    Fader, FrameScan, GuiParam, HeadphoneParam, InputSourceParam, Layout, MasterParam,
    MixMinusesParam, MixOutput, NetworkParam, Node, OutputParam, PadParam, PlayerParam, RadioParam,
    RadioRxParam, RadioTxParam, RcSyncMixParam, RecorderParam, RecordingParam, RecordingsParam,
    ShowControlParam, ShowParam, Source, StorageVolumeParam, StreamerXMixPresetParam,
    StreamerXStreamMixParam, SystemParam, ThemeParam, Value, WifiScanResultParam,
};

const RODE_VID: u16 = 0x19F7;
const PID_PRO2: u16 = 0x0094;
const PID_DUO: u16 = 0x0095;
const PID_DUO_ALT: u16 = 0x0050;

type Shared<T> = Arc<Mutex<T>>;
type DynErr = Box<dyn std::error::Error + Send + Sync>;

/// Write side of a connected transport: hands one JUCE body over to the
/// transport's framing module and onto the wire. Cloneable so the REPL can
/// hold one writer while other threads keep their own [`Arc`] to the
/// underlying file / socket.
type Writer = Arc<dyn Fn(Vec<u8>) -> Result<(), DynErr> + Send + Sync>;

enum TransportArg {
    Usb(Option<PathBuf>),
    Tcp(String),
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<TransportArg, DynErr> {
    let mut args = std::env::args().skip(1);
    match (args.next(), args.next(), args.next()) {
        (None, _, _) => Ok(TransportArg::Usb(None)),
        (Some(t), None, _) if t == "usb" => Ok(TransportArg::Usb(None)),
        (Some(t), Some(p), None) if t == "usb" => Ok(TransportArg::Usb(Some(PathBuf::from(p)))),
        (Some(t), Some(addr), None) if t == "tcp" => Ok(TransportArg::Tcp(addr)),
        _ => Err("usage: cli [usb [/dev/hidrawN] | tcp <host:port>]".into()),
    }
}

fn run() -> Result<(), DynErr> {
    let arg = parse_args()?;
    let layout: Shared<Option<Layout>> = Arc::new(Mutex::new(None));
    let root: Shared<Option<Node>> = Arc::new(Mutex::new(None));
    let watch = Arc::new(AtomicBool::new(true));

    let writer = match arg {
        TransportArg::Usb(path) => start_usb(path, &layout, &root, &watch)?,
        TransportArg::Tcp(addr) => start_tcp(&addr, &layout, &root, &watch)?,
    };

    // Give the reader a beat to ingest the first fullSync so `layout` is
    // probably populated before the user's first command.
    thread::sleep(Duration::from_millis(250));

    repl(&writer, &layout, &root, &watch)
}

// ──────────────────────────────────────────────────────────────────────────
// Transport setup
// ──────────────────────────────────────────────────────────────────────────

fn start_usb(
    explicit: Option<PathBuf>,
    layout: &Shared<Option<Layout>>,
    root: &Shared<Option<Node>>,
    watch: &Arc<AtomicBool>,
) -> Result<Writer, DynErr> {
    let path = match explicit {
        Some(p) => p,
        None => {
            let (p, model) = find_device()?.ok_or(
                "no RODECaster Pro II / Duo on any /dev/hidraw* (VID 19f7 PID 0094/0095). \
                 Is the device plugged in and powered?",
            )?;
            eprintln!("usb: {} ({})", p.display(), model_label(model));
            p
        }
    };
    let file = Arc::new(open_hidraw(&path)?);
    // USB needs an explicit handshake; the device stays silent until it sees
    // [`usb::HANDSHAKE_BODY`].
    write_usb(&file, usb::HANDSHAKE_BODY.to_vec())?;

    let reader_file = Arc::clone(&file);
    let reader_layout = Arc::clone(layout);
    let reader_root = Arc::clone(root);
    let reader_watch = Arc::clone(watch);
    thread::spawn(move || usb_reader_loop(reader_file, reader_layout, reader_root, reader_watch));

    let writer_file = Arc::clone(&file);
    Ok(Arc::new(move |body| write_usb(&writer_file, body)))
}

fn start_tcp(
    addr: &str,
    layout: &Shared<Option<Layout>>,
    root: &Shared<Option<Node>>,
    watch: &Arc<AtomicBool>,
) -> Result<Writer, DynErr> {
    eprintln!("tcp: connecting to {addr}");
    let stream = Arc::new(TcpStream::connect(addr)?);
    eprintln!("tcp: connected");
    // No handshake on TCP: the device starts streaming the
    // fullSync as soon as a client connects.

    let reader_stream = Arc::clone(&stream);
    let reader_layout = Arc::clone(layout);
    let reader_root = Arc::clone(root);
    let reader_watch = Arc::clone(watch);
    thread::spawn(move || tcp_reader_loop(reader_stream, reader_layout, reader_root, reader_watch));

    let writer_stream = Arc::clone(&stream);
    Ok(Arc::new(move |body| write_tcp(&writer_stream, body)))
}

// ──────────────────────────────────────────────────────────────────────────
// Device discovery
// ──────────────────────────────────────────────────────────────────────────

fn find_device() -> Result<Option<(PathBuf, DeviceModel)>, DynErr> {
    let class = Path::new("/sys/class/hidraw");
    if !class.exists() {
        return Err("no /sys/class/hidraw on this system; this example is Linux-only".into());
    }
    for entry in fs::read_dir(class)? {
        let entry = entry?;
        let uevent = entry.path().join("device/uevent");
        let Ok(text) = fs::read_to_string(&uevent) else {
            continue;
        };
        // Format: HID_ID=0003:000019F7:00000094
        let Some(line) = text.lines().find(|l| l.starts_with("HID_ID=")) else {
            continue;
        };
        let parts: Vec<&str> = line.trim_start_matches("HID_ID=").split(':').collect();
        if parts.len() != 3 {
            continue;
        }
        let vid = u32::from_str_radix(parts[1], 16).unwrap_or(0) as u16;
        let pid = u32::from_str_radix(parts[2], 16).unwrap_or(0) as u16;
        if vid != RODE_VID {
            continue;
        }
        let model = match pid {
            PID_PRO2 => DeviceModel::Pro2,
            PID_DUO | PID_DUO_ALT => DeviceModel::Duo,
            _ => continue,
        };
        let dev = PathBuf::from("/dev").join(entry.file_name());
        return Ok(Some((dev, model)));
    }
    Ok(None)
}

fn open_hidraw(path: &Path) -> Result<File, DynErr> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()).into())
}

fn model_label(m: DeviceModel) -> &'static str {
    match m {
        DeviceModel::Pro2 => "RODECaster Pro II",
        DeviceModel::Duo => "RODECaster Duo",
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Transport I/O: two pairs (write + reader_loop), one per framing
// ──────────────────────────────────────────────────────────────────────────

/// Wrap one JUCE body into [`usb::Packet`], chunk into HID reports, write each
/// report as one syscall (the kernel wants one report per write).
fn write_usb(file: &File, body: Vec<u8>) -> Result<(), DynErr> {
    let bytes = usb::Packet::new(body).to_bytes();
    let mut f: &File = file;
    for chunk in bytes.chunks(usb::REPORT_SIZE) {
        f.write_all(chunk)?;
    }
    Ok(())
}

/// Wrap one JUCE body into [`frame::Packet`] (magic + length + body) and write
/// the whole frame in one stream write.
fn write_tcp(stream: &TcpStream, body: Vec<u8>) -> Result<(), DynErr> {
    let bytes = frame::Packet::new(body).to_bytes();
    let mut s: &TcpStream = stream;
    s.write_all(&bytes)?;
    Ok(())
}

fn usb_reader_loop(
    file: Arc<File>,
    layout: Shared<Option<Layout>>,
    root: Shared<Option<Node>>,
    watch: Arc<AtomicBool>,
) {
    let mut buf = Vec::<u8>::with_capacity(usb::REPORT_SIZE * 4);
    let mut report = [0u8; usb::REPORT_SIZE];
    loop {
        let mut f: &File = &file;
        let n = match f.read(&mut report) {
            Ok(0) => return, // EOF: device went away or main dropped its handle
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        buf.extend_from_slice(&report[..n]);
        loop {
            match usb::scan_frame(&buf) {
                FrameScan::Incomplete => break,
                FrameScan::Desync => {
                    // One report's worth is junk; drop and rescan.
                    let drop = usb::REPORT_SIZE.min(buf.len());
                    buf.drain(..drop);
                }
                FrameScan::Complete { len } => {
                    let Some((packet, consumed)) = usb::Packet::from_bytes(&buf[..len]) else {
                        buf.drain(..len);
                        continue;
                    };
                    buf.drain(..consumed);
                    handle_body(packet.payload, &layout, &root, &watch);
                }
            }
        }
    }
}

fn tcp_reader_loop(
    stream: Arc<TcpStream>,
    layout: Shared<Option<Layout>>,
    root: Shared<Option<Node>>,
    watch: Arc<AtomicBool>,
) {
    let mut buf = Vec::<u8>::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let mut s: &TcpStream = &stream;
        let n = match s.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        buf.extend_from_slice(&chunk[..n]);
        loop {
            match frame::scan_frame(&buf) {
                FrameScan::Incomplete => break,
                FrameScan::Desync => {
                    // TCP frames are byte-aligned (no report stride);
                    // slide one byte and rescan to resync on a fresh magic.
                    buf.drain(..1);
                }
                FrameScan::Complete { len } => {
                    let Some((packet, consumed)) = frame::Packet::from_bytes(&buf[..len]) else {
                        buf.drain(..len);
                        continue;
                    };
                    buf.drain(..consumed);
                    handle_body(packet.payload, &layout, &root, &watch);
                }
            }
        }
    }
}

fn handle_body(
    body: Vec<u8>,
    layout: &Shared<Option<Layout>>,
    root: &Shared<Option<Node>>,
    watch: &AtomicBool,
) {
    // 0x02 = JUCE fullSync. Build a fresh Layout from the parsed tree so
    // subsequent change-frames decode against an up-to-date address map.
    if body.first() == Some(&0x02) {
        if let Some(parsed_root) = parse_valuetree(&body) {
            match Layout::from_full_sync(&parsed_root) {
                Ok(new_layout) => {
                    println!(
                        "\n[layout] fullSync received: {} model, {} faders, {} sources, {} mixes",
                        model_label(new_layout.model()),
                        new_layout.fader_count(),
                        new_layout.source_count(),
                        new_layout.mix_count_per_source(),
                    );
                    *layout.lock().unwrap() = Some(new_layout);
                    *root.lock().unwrap() = Some(parsed_root);
                    if watch.load(Ordering::Relaxed) {
                        // InitialState bursts can flood the terminal; print only
                        // the count, not every property.
                        if let Some(l) = layout.lock().unwrap().as_ref() {
                            if let Some(DeviceEvent::InitialState(events)) = decode_event(&body, l)
                            {
                                println!("[initial-state] {} properties", events.len());
                            }
                        }
                    }
                    reprompt();
                }
                Err(e) => {
                    println!("\n[layout] fullSync decode failed: {e:?}");
                    reprompt();
                }
            }
        }
        return;
    }

    let guard = layout.lock().unwrap();
    let Some(l) = guard.as_ref() else {
        if watch.load(Ordering::Relaxed) {
            println!("\n[event] (no layout yet, {} bytes)", body.len());
            reprompt();
        }
        return;
    };
    if let Some(event) = decode_event(&body, l) {
        if watch.load(Ordering::Relaxed) {
            print_event(&event);
        }
    }
}

fn print_event(event: &DeviceEvent) {
    match event {
        DeviceEvent::InitialState(v) => {
            println!("\n[event] InitialState ({} properties)", v.len());
        }
        other => {
            println!("\n[event] {other:?}");
        }
    }
    reprompt();
}

fn reprompt() {
    print!("> ");
    let _ = io::stdout().flush();
}

// ──────────────────────────────────────────────────────────────────────────
// REPL
// ──────────────────────────────────────────────────────────────────────────

fn repl(
    writer: &Writer,
    layout: &Shared<Option<Layout>>,
    root: &Shared<Option<Node>>,
    watch: &AtomicBool,
) -> Result<(), DynErr> {
    let stdin = io::stdin();
    reprompt();
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            reprompt();
            continue;
        }
        match dispatch(trimmed, writer, layout, root, watch) {
            Ok(Outcome::Continue) => {}
            Ok(Outcome::Quit) => return Ok(()),
            Err(e) => println!("err: {e}"),
        }
        reprompt();
    }
    Ok(())
}

enum Outcome {
    Continue,
    Quit,
}

fn dispatch(
    line: &str,
    writer: &Writer,
    layout: &Shared<Option<Layout>>,
    root: &Shared<Option<Node>>,
    watch: &AtomicBool,
) -> Result<Outcome, String> {
    let mut tok = line.split_whitespace();
    let head = tok.next().unwrap();
    let rest: Vec<&str> = tok.collect();
    match head {
        "help" | "?" => {
            print_help();
            Ok(Outcome::Continue)
        }
        "quit" | "exit" => Ok(Outcome::Quit),
        "layout" => {
            print_layout(layout);
            Ok(Outcome::Continue)
        }
        "dump-cell" => {
            dump_cell(layout, root, &rest)?;
            Ok(Outcome::Continue)
        }
        "raw-press" => raw_request(writer, layout, &rest, RawValue::Press),
        "raw-release" => raw_request(writer, layout, &rest, RawValue::Release),
        "raw-bool" => {
            let on = parse_bool(rest.get(3).copied().unwrap_or(""))?;
            raw_request(writer, layout, &rest[..3], RawValue::Bool(on))
        }
        "raw-write" => raw_write(writer, &rest),
        "refresh" => {
            writer(usb::HANDSHAKE_BODY.to_vec()).map_err(|e| format!("write: {e}"))?;
            println!("sent handshake; fresh fullSync incoming");
            Ok(Outcome::Continue)
        }
        "dump-schema" => {
            dump_schema(root)?;
            Ok(Outcome::Continue)
        }
        "dump-node" => {
            let name = rest
                .first()
                .copied()
                .ok_or_else(|| "usage: dump-node <NODE_NAME>".to_string())?;
            dump_node(root, name)?;
            Ok(Outcome::Continue)
        }
        "watch" => {
            let on = parse_bool(rest.first().copied().unwrap_or("on"))?;
            watch.store(on, Ordering::Relaxed);
            println!("watch {}", if on { "on" } else { "off" });
            Ok(Outcome::Continue)
        }
        "mute" => one_shot(writer, layout, parse_set_fader_mute(&rest)?),
        "cue" => one_shot(writer, layout, parse_set_fader_cue(&rest)?),
        "level" => one_shot(writer, layout, parse_set_fader_level(&rest)?),
        "assign" => one_shot(writer, layout, parse_assign_fader(&rest)?),
        "mix-disable" => one_shot(writer, layout, parse_mix_disable(&rest)?),
        "mix-mute" => one_shot(writer, layout, parse_mix_mute(&rest)?),
        "link" => one_shot(writer, layout, parse_link(&rest)?),
        "unlink" => one_shot(writer, layout, parse_unlink(&rest)?),
        "link-callme" => one_shot(writer, layout, parse_link_callme(&rest)?),
        "unlink-callme" => one_shot(writer, layout, parse_unlink_callme(&rest)?),
        "screen" => one_shot(writer, layout, Command::ScreenTouched),
        "skip-setup" | "setup-skip" => one_shot(writer, layout, parse_setup_skip(&rest)?),
        "power" => {
            print!("really power off? type 'yes' to confirm: ");
            io::stdout().flush().ok();
            let mut s = String::new();
            io::stdin().read_line(&mut s).ok();
            if s.trim() == "yes" {
                one_shot(writer, layout, Command::PowerOff)
            } else {
                println!("cancelled");
                Ok(Outcome::Continue)
            }
        }
        "set" => one_shot(writer, layout, parse_set(&rest)?),
        other => Err(format!("unknown command: {other}. try `help`")),
    }
}

fn one_shot(
    writer: &Writer,
    layout: &Shared<Option<Layout>>,
    cmd: Command,
) -> Result<Outcome, String> {
    let guard = layout.lock().unwrap();
    let layout_ref = guard
        .as_ref()
        .ok_or_else(|| "no Layout yet; wait for the fullSync print".to_string())?;
    let frames = cmd
        .encode(layout_ref)
        .map_err(|e| format!("encode: {e:?}"))?;
    drop(guard);
    let n = frames.len();
    for body in frames {
        writer(body).map_err(|e| format!("write: {e}"))?;
    }
    println!("sent {n} frame{}", if n == 1 { "" } else { "s" });
    Ok(Outcome::Continue)
}

// ──────────────────────────────────────────────────────────────────────────
// Parsers
// ──────────────────────────────────────────────────────────────────────────

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Ok(true),
        "off" | "false" | "0" | "no" => Ok(false),
        other => Err(format!("expected on/off, got {other}")),
    }
}

fn parse_fader(s: &str) -> Result<Fader, String> {
    match s.to_ascii_lowercase().as_str() {
        "p1" => Ok(Fader::Physical1),
        "p2" => Ok(Fader::Physical2),
        "p3" => Ok(Fader::Physical3),
        "p4" => Ok(Fader::Physical4),
        "p5" => Ok(Fader::Physical5),
        "p6" => Ok(Fader::Physical6),
        "v1" => Ok(Fader::Virtual1),
        "v2" => Ok(Fader::Virtual2),
        "v3" => Ok(Fader::Virtual3),
        "v4" => Ok(Fader::Virtual4),
        "v5" => Ok(Fader::Virtual5),
        other => Err(format!("expected fader p1..p6 / v1..v5, got {other}")),
    }
}

fn parse_source(s: &str) -> Result<Source, String> {
    match s.to_ascii_lowercase().as_str() {
        "combo1" => Ok(Source::Combo1),
        "combo2" => Ok(Source::Combo2),
        "combo3" => Ok(Source::Combo3),
        "combo4" => Ok(Source::Combo4),
        "combo1_2" | "combo12" => Ok(Source::Combo1_2),
        "combo2_3" | "combo23" => Ok(Source::Combo2_3),
        "combo3_4" | "combo34" => Ok(Source::Combo3_4),
        "usb1" => Ok(Source::Usb1),
        "chat" => Ok(Source::Chat),
        "usb2" => Ok(Source::Usb2),
        "bt" | "bluetooth" => Ok(Source::Bluetooth),
        "pad" | "soundpad" => Ok(Source::SoundPad),
        "vgame" | "virtualgame" => Ok(Source::VirtualGame),
        "vmusic" | "virtualmusic" => Ok(Source::VirtualMusic),
        "va" | "virtuala" => Ok(Source::VirtualA),
        "vb" | "virtualb" => Ok(Source::VirtualB),
        "callme1" => Ok(Source::CallMe1),
        "callme2" => Ok(Source::CallMe2),
        "callme3" => Ok(Source::CallMe3),
        other => Err(format!("unknown source: {other}")),
    }
}

fn parse_mix(s: &str) -> Result<MixOutput, String> {
    match s.to_ascii_lowercase().as_str() {
        "hp1" | "headphone1" => Ok(MixOutput::Headphone1),
        "hp2" | "headphone2" => Ok(MixOutput::Headphone2),
        "hp3" | "headphone3" => Ok(MixOutput::Headphone3),
        "hp4" | "headphone4" => Ok(MixOutput::Headphone4),
        "spk" | "speaker" => Ok(MixOutput::Speaker),
        "rec" | "recording" => Ok(MixOutput::Recording),
        "bt" | "bluetooth" => Ok(MixOutput::Bluetooth),
        "usb1" => Ok(MixOutput::Usb1),
        "chat" => Ok(MixOutput::Chat),
        "usb2" => Ok(MixOutput::Usb2),
        "callme1" => Ok(MixOutput::CallMe1),
        "callme2" => Ok(MixOutput::CallMe2),
        "callme3" => Ok(MixOutput::CallMe3),
        other => Err(format!("unknown mix output: {other}")),
    }
}

fn need_args<'a>(args: &'a [&'a str], n: usize, usage: &str) -> Result<(), String> {
    if args.len() < n {
        Err(format!("usage: {usage}"))
    } else {
        Ok(())
    }
}

fn parse_set_fader_mute(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "mute <fader> <on|off>")?;
    Ok(Command::SetFaderMute {
        fader: parse_fader(args[0])?,
        mute: parse_bool(args[1])?,
    })
}

fn parse_set_fader_cue(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "cue <fader> <on|off>")?;
    Ok(Command::SetFaderCue {
        fader: parse_fader(args[0])?,
        enable: parse_bool(args[1])?,
    })
}

fn parse_set_fader_level(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "level <fader> <0..127>")?;
    let level: u8 = args[1]
        .parse()
        .map_err(|_| format!("level not 0..127: {}", args[1]))?;
    Ok(Command::SetFaderLevel {
        fader: parse_fader(args[0])?,
        level,
    })
}

fn parse_assign_fader(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "assign <fader> <source|clear>")?;
    let fader = parse_fader(args[0])?;
    let source = if args[1].eq_ignore_ascii_case("clear") || args[1].eq_ignore_ascii_case("none") {
        None
    } else {
        Some(parse_source(args[1])?)
    };
    Ok(Command::AssignFaderSource { fader, source })
}

fn parse_mix_disable(args: &[&str]) -> Result<Command, String> {
    need_args(args, 3, "mix-disable <source> <mix> <on|off>")?;
    Ok(Command::SetMixDisabled {
        source: parse_source(args[0])?,
        mix: parse_mix(args[1])?,
        disabled: parse_bool(args[2])?,
    })
}

fn parse_mix_mute(args: &[&str]) -> Result<Command, String> {
    need_args(args, 3, "mix-mute <source> <mix> <on|off>")?;
    Ok(Command::SetMixMute {
        source: parse_source(args[0])?,
        mix: parse_mix(args[1])?,
        mute: parse_bool(args[2])?,
    })
}

fn parse_link(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "link <source> <mix>")?;
    Ok(Command::LinkMix {
        source: parse_source(args[0])?,
        mix: parse_mix(args[1])?,
    })
}

fn parse_unlink(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "unlink <source> <mix>")?;
    Ok(Command::UnlinkMix {
        source: parse_source(args[0])?,
        mix: parse_mix(args[1])?,
    })
}

fn parse_link_callme(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "link-callme <callme1..3> <mix>")?;
    let source = parse_source(args[0])?;
    if !matches!(source, Source::CallMe1 | Source::CallMe2 | Source::CallMe3) {
        return Err("link-callme needs a CallMe source (callme1..callme3)".into());
    }
    Ok(Command::LinkCallMe {
        source,
        mix: parse_mix(args[1])?,
    })
}

fn parse_unlink_callme(args: &[&str]) -> Result<Command, String> {
    need_args(args, 2, "unlink-callme <callme1..3> <mix>")?;
    let source = parse_source(args[0])?;
    if !matches!(source, Source::CallMe1 | Source::CallMe2 | Source::CallMe3) {
        return Err("unlink-callme needs a CallMe source (callme1..callme3)".into());
    }
    Ok(Command::UnlinkCallMe {
        source,
        mix: parse_mix(args[1])?,
    })
}

fn parse_setup_skip(args: &[&str]) -> Result<Command, String> {
    let lang = args
        .first()
        .copied()
        .filter(|s| *s != "none")
        .map(|s| s.to_string());
    let tz = args
        .get(1..)
        .map(|a| a.join(" "))
        .filter(|s| !s.is_empty() && s != "none");
    Ok(Command::SetupSkip {
        language: lang,
        timezone: tz,
    })
}

/// `set <family> [key...] <wire-name> <value-kind> <value...>`
///
/// Families with no addressing key: master / output / ducker / recorder /
/// player / gui / system. Per-instance families need a key:
///   - channel        <fader>
///   - inputsource    <source>
///   - headphone      <0..N>     (N = `layout.headphone_count()`)
///   - effects        <0..N>     (N = `layout.effects_count()`)
///   - pad            <0..N>     (N = `layout.pad_count()`)
fn parse_set(args: &[&str]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("usage: set <family> [key] <wire-name> <kind> <value...>".into());
    }
    let family = args[0].to_ascii_lowercase();
    let rest = &args[1..];
    match family.as_str() {
        "master" => {
            need_args(rest, 3, "set master <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetMasterParam {
                param: MasterParam::from_name(rest[0]),
                value,
            })
        }
        "output" => {
            need_args(rest, 3, "set output <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetOutputParam {
                param: OutputParam::from_name(rest[0]),
                value,
            })
        }
        "ducker" => {
            need_args(rest, 3, "set ducker <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetDuckerParam {
                param: DuckerParam::from_name(rest[0]),
                value,
            })
        }
        "recorder" => {
            need_args(rest, 3, "set recorder <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetRecorderParam {
                param: RecorderParam::from_name(rest[0]),
                value,
            })
        }
        "player" => {
            need_args(rest, 3, "set player <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetPlayerParam {
                param: PlayerParam::from_name(rest[0]),
                value,
            })
        }
        "gui" => {
            need_args(rest, 3, "set gui <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetGuiParam {
                param: GuiParam::from_name(rest[0]),
                value,
            })
        }
        "system" => {
            need_args(rest, 3, "set system <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetSystemParam {
                param: SystemParam::from_name(rest[0]),
                value,
            })
        }
        "channel" => {
            need_args(rest, 4, "set channel <fader> <wire-name> <kind> <value>")?;
            let fader = parse_fader(rest[0])?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetChannelParam {
                fader,
                param: ChannelParam::from_name(rest[1]),
                value,
            })
        }
        "inputsource" | "input-source" | "input_source" => {
            need_args(
                rest,
                4,
                "set inputsource <source> <wire-name> <kind> <value>",
            )?;
            let source = parse_source(rest[0])?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetInputSourceParam {
                source,
                param: InputSourceParam::from_name(rest[1]),
                value,
            })
        }
        "headphone" => {
            need_args(rest, 4, "set headphone <0..N> <wire-name> <kind> <value>")?;
            let headphone: u8 = rest[0]
                .parse()
                .map_err(|_| format!("headphone index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetHeadphoneParam {
                headphone,
                param: HeadphoneParam::from_name(rest[1]),
                value,
            })
        }
        "effects" => {
            need_args(rest, 4, "set effects <0..N> <wire-name> <kind> <value>")?;
            let effects: u8 = rest[0]
                .parse()
                .map_err(|_| format!("effects index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetEffectsParam {
                effects,
                param: EffectsParam::from_name(rest[1]),
                value,
            })
        }
        "pad" => {
            need_args(rest, 4, "set pad <0..N> <wire-name> <kind> <value>")?;
            let pad: u8 = rest[0]
                .parse()
                .map_err(|_| format!("pad index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetPadParam {
                pad,
                param: PadParam::from_name(rest[1]),
                value,
            })
        }
        "network" => {
            need_args(rest, 3, "set network <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetNetworkParam {
                param: NetworkParam::from_name(rest[0]),
                value,
            })
        }
        "audio" => {
            need_args(rest, 3, "set audio <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetAudioParam {
                param: AudioParam::from_name(rest[0]),
                value,
            })
        }
        "build" => {
            need_args(rest, 3, "set build <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetBuildParam {
                param: BuildParam::from_name(rest[0]),
                value,
            })
        }
        "app" => {
            need_args(rest, 3, "set app <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetAppParam {
                param: AppParam::from_name(rest[0]),
                value,
            })
        }
        "theme" => {
            need_args(rest, 3, "set theme <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetThemeParam {
                param: ThemeParam::from_name(rest[0]),
                value,
            })
        }
        "currentshow" | "current-show" => {
            need_args(rest, 3, "set currentshow <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetCurrentShowParam {
                param: CurrentShowParam::from_name(rest[0]),
                value,
            })
        }
        "showcontrol" | "show-control" => {
            need_args(rest, 3, "set showcontrol <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetShowControlParam {
                param: ShowControlParam::from_name(rest[0]),
                value,
            })
        }
        "recordings" => {
            need_args(rest, 3, "set recordings <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetRecordingsParam {
                param: RecordingsParam::from_name(rest[0]),
                value,
            })
        }
        "radio" => {
            need_args(rest, 3, "set radio <wire-name> <kind> <value>")?;
            let value = parse_value(&rest[1..])?;
            Ok(Command::SetRadioParam {
                param: RadioParam::from_name(rest[0]),
                value,
            })
        }
        "show" => {
            need_args(rest, 4, "set show <0..N> <wire-name> <kind> <value>")?;
            let show: u8 = rest[0]
                .parse()
                .map_err(|_| format!("show index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetShowParam {
                show,
                param: ShowParam::from_name(rest[1]),
                value,
            })
        }
        "recording" => {
            need_args(rest, 4, "set recording <0..N> <wire-name> <kind> <value>")?;
            let recording: u8 = rest[0]
                .parse()
                .map_err(|_| format!("recording index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetRecordingParam {
                recording,
                param: RecordingParam::from_name(rest[1]),
                value,
            })
        }
        "storagevolume" | "storage-volume" => {
            need_args(
                rest,
                4,
                "set storagevolume <0..N> <wire-name> <kind> <value>",
            )?;
            let volume: u8 = rest[0]
                .parse()
                .map_err(|_| format!("storage volume index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetStorageVolumeParam {
                volume,
                param: StorageVolumeParam::from_name(rest[1]),
                value,
            })
        }
        "radiotx" | "radio-tx" => {
            need_args(rest, 4, "set radiotx <0..N> <wire-name> <kind> <value>")?;
            let tx: u8 = rest[0]
                .parse()
                .map_err(|_| format!("radio tx index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetRadioTxParam {
                tx,
                param: RadioTxParam::from_name(rest[1]),
                value,
            })
        }
        "radiorx" | "radio-rx" => {
            need_args(rest, 4, "set radiorx <0..N> <wire-name> <kind> <value>")?;
            let rx: u8 = rest[0]
                .parse()
                .map_err(|_| format!("radio rx index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetRadioRxParam {
                rx,
                param: RadioRxParam::from_name(rest[1]),
                value,
            })
        }
        "wifiscanresult" | "wifi-scan-result" => {
            need_args(
                rest,
                4,
                "set wifiscanresult <0..N> <wire-name> <kind> <value>",
            )?;
            let slot: u8 = rest[0]
                .parse()
                .map_err(|_| format!("wifi scan result slot index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetWifiScanResultParam {
                slot,
                param: WifiScanResultParam::from_name(rest[1]),
                value,
            })
        }
        "streamerxmixpreset" | "streamerx-mix-preset" => {
            need_args(
                rest,
                4,
                "set streamerxmixpreset <0..N> <wire-name> <kind> <value>",
            )?;
            let preset: u8 = rest[0]
                .parse()
                .map_err(|_| format!("streamerx mix preset index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetStreamerXMixPresetParam {
                preset,
                param: StreamerXMixPresetParam::from_name(rest[1]),
                value,
            })
        }
        "streamerxstreammix" | "streamerx-stream-mix" => {
            need_args(
                rest,
                4,
                "set streamerxstreammix <0..N> <wire-name> <kind> <value>",
            )?;
            let stream: u8 = rest[0]
                .parse()
                .map_err(|_| format!("streamerx stream mix index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetStreamerXStreamMixParam {
                stream,
                param: StreamerXStreamMixParam::from_name(rest[1]),
                value,
            })
        }
        "rcsyncmix" | "rcsync-mix" => {
            need_args(rest, 4, "set rcsyncmix <0..N> <wire-name> <kind> <value>")?;
            let mix: u8 = rest[0]
                .parse()
                .map_err(|_| format!("rcsync mix index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetRcSyncMixParam {
                mix,
                param: RcSyncMixParam::from_name(rest[1]),
                value,
            })
        }
        "mixminuses" | "mix-minuses" => {
            need_args(rest, 4, "set mixminuses <0..N> <wire-name> <kind> <value>")?;
            let minuses: u8 = rest[0]
                .parse()
                .map_err(|_| format!("mix minuses index not numeric: {}", rest[0]))?;
            let value = parse_value(&rest[2..])?;
            Ok(Command::SetMixMinusesParam {
                minuses,
                param: MixMinusesParam::from_name(rest[1]),
                value,
            })
        }
        other => Err(format!("unknown family: {other}")),
    }
}

/// Parse `<kind> <value...>` into a [`Value`]. Kinds: int / i64 / bool / dbl / str.
fn parse_value(args: &[&str]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("value needs <kind> <value>; kinds: int i64 bool dbl str".into());
    }
    let kind = args[0].to_ascii_lowercase();
    let payload = args[1..].join(" ");
    match kind.as_str() {
        "int" => payload
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|e| format!("int: {e}")),
        "i64" => payload
            .parse::<i64>()
            .map(Value::Int64)
            .map_err(|e| format!("i64: {e}")),
        "bool" => parse_bool(&payload).map(Value::Bool),
        "dbl" | "double" | "f64" => payload
            .parse::<f64>()
            .map(Value::Double)
            .map_err(|e| format!("dbl: {e}")),
        "str" | "string" => Ok(Value::String(payload)),
        other => Err(format!("unknown value kind: {other}")),
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Printers
// ──────────────────────────────────────────────────────────────────────────

/// What to write into the `mixLinkRequest` / `mixUnlinkRequest` property for a
/// probe: used to deconstruct whether the press blob, the release blob, a
/// plain `Bool`, or any other shape is sufficient to flip a cell.
enum RawValue {
    /// Single Binary frame: `[01,01,02,01,01,02]`. The touchscreen's first frame.
    Press,
    /// Single Binary frame: `[01,01,03,01,01,03]`. The touchscreen's second frame.
    Release,
    /// Single Bool frame: the property's natural at-rest type per fullSync dumps.
    Bool(bool),
}

/// `raw-press|raw-release|raw-bool <source> <mix> <link|unlink>`: send ONE
/// frame to `mixLinkRequest` (link) or `mixUnlinkRequest` (unlink) carrying
/// the chosen payload. No press/release pair, no enable/unmute, just the raw
/// property write. Probes whether the device gates a state change on the
/// payload type or the property name alone.
fn raw_request(
    writer: &Writer,
    layout: &Shared<Option<Layout>>,
    args: &[&str],
    raw: RawValue,
) -> Result<Outcome, String> {
    if args.len() < 3 {
        return Err("usage: raw-press|raw-release <source> <mix> <link|unlink> \
                    OR raw-bool <source> <mix> <link|unlink> <on|off>"
            .into());
    }
    let source = parse_source(args[0])?;
    let mix = parse_mix(args[1])?;
    let property = match args[2] {
        "link" => "mixLinkRequest",
        "unlink" => "mixUnlinkRequest",
        other => return Err(format!("expected link|unlink, got {other}")),
    };
    let value = match raw {
        RawValue::Press => Value::Binary(vec![0x01, 0x01, 0x02, 0x01, 0x01, 0x02]),
        RawValue::Release => Value::Binary(vec![0x01, 0x01, 0x03, 0x01, 0x01, 0x03]),
        RawValue::Bool(b) => Value::Bool(b),
    };
    let layout_guard = layout.lock().unwrap();
    let layout_ref = layout_guard
        .as_ref()
        .ok_or_else(|| "no Layout yet".to_string())?;
    let path = layout_ref
        .mix_cell_path(source.to_protocol(), mix.to_protocol())
        .ok_or_else(|| "bad cell".to_string())?;
    drop(layout_guard);
    let body = change_frame::encode_property_changed(&path, property, &value);
    writer(body).map_err(|e| format!("write: {e}"))?;
    println!("sent 1 frame: {property} = {value:?}");
    Ok(Outcome::Continue)
}

/// `raw-write <PATH> <PROP_NAME> <KIND> <VALUE...>`: send a raw
/// propertyChanged frame with arbitrary path, property name, and value.
/// PATH is comma-separated `u32`s (e.g. `359,0`). KIND is one of `int`,
/// `i64`, `bool`, `dbl`, `str`. For `str`, the remainder of the line is the
/// string (allowing spaces). This is the fuzzing / probing primitive:
/// bypasses every typed `Command` and lets you write anything to anywhere.
fn raw_write(writer: &Writer, args: &[&str]) -> Result<Outcome, String> {
    if args.len() < 3 {
        return Err(
            "usage: raw-write <path,commas,between,u32s> <prop_name> <kind> [<value...>] \
             (value defaults to empty for str)"
                .into(),
        );
    }
    let path: Vec<u32> = args[0]
        .split(',')
        .map(|s| s.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("bad path (need comma-separated u32s): {e}"))?;
    let prop_name = args[1];
    let kind = args[2].to_ascii_lowercase();
    let payload = args.get(3..).map(|s| s.join(" ")).unwrap_or_default();
    let value = match kind.as_str() {
        "int" => Value::Int(payload.parse::<i64>().map_err(|e| format!("int: {e}"))?),
        "i64" => Value::Int64(payload.parse::<i64>().map_err(|e| format!("i64: {e}"))?),
        "bool" => Value::Bool(parse_bool(&payload)?),
        "dbl" | "f64" | "double" => {
            Value::Double(payload.parse::<f64>().map_err(|e| format!("dbl: {e}"))?)
        }
        "str" | "string" => Value::String(payload),
        other => return Err(format!("unknown kind: {other}")),
    };
    let body = change_frame::encode_property_changed(&path, prop_name, &value);
    writer(body).map_err(|e| format!("write: {e}"))?;
    println!("sent raw {prop_name} = {value:?} at path {path:?}");
    Ok(Outcome::Continue)
}

/// `dump-node <NAME>`: find the first node in the cached fullSync tree whose
/// name matches, and print all its properties + values (with the current
/// wire types visible). Useful for reading singleton node state
/// (`NETWORK`, `SYSTEM`, `OUTPUT`, etc.) without waiting for a change event.
fn dump_node(root: &Shared<Option<Node>>, name: &str) -> Result<(), String> {
    let guard = root.lock().unwrap();
    let r = guard
        .as_ref()
        .ok_or_else(|| "no fullSync parsed yet".to_string())?;
    fn find<'a>(n: &'a Node, target: &str, path: &mut Vec<u32>) -> Option<(&'a Node, Vec<u32>)> {
        if n.name == target {
            return Some((n, path.clone()));
        }
        for (i, c) in n.children.iter().enumerate() {
            path.push(i as u32);
            if let Some(found) = find(c, target, path) {
                return Some(found);
            }
            path.pop();
        }
        None
    }
    let mut path = Vec::new();
    let (node, node_path) =
        find(r, name, &mut path).ok_or_else(|| format!("no node named {name}"))?;
    println!(
        "[node] <{}> at path {:?} ({} properties, {} children)",
        node.name,
        node_path,
        node.properties.len(),
        node.children.len()
    );
    for p in &node.properties {
        println!("  {} = {:?}", p.name, p.value);
    }
    Ok(())
}

/// `dump-schema`: walk the cached fullSync tree and print every unique
/// `(node_name, property_name)` pair found. Used to enumerate the wire's
/// total property surface for completeness auditing.
fn dump_schema(root: &Shared<Option<Node>>) -> Result<(), String> {
    let guard = root.lock().unwrap();
    let r = guard
        .as_ref()
        .ok_or_else(|| "no fullSync parsed yet".to_string())?;
    let mut seen: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    fn walk(
        n: &Node,
        seen: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    ) {
        let entry = seen.entry(n.name.clone()).or_default();
        for p in &n.properties {
            entry.insert(p.name.clone());
        }
        for c in &n.children {
            walk(c, seen);
        }
    }
    walk(r, &mut seen);
    println!("[schema] {} unique node types", seen.len());
    for (node, props) in &seen {
        println!("  <{node}> ({} props)", props.len());
        for p in props {
            println!("    - {p}");
        }
    }
    Ok(())
}

/// `dump-cell <source> <mix>`: find the MIX node for that (source, mix) cell
/// in the cached parsed root and print all of its properties + values, raw.
/// No interpretation, no decode. Lets you see what `mixLinkRequest` and friends
/// actually look like at rest, and how they differ across cells.
fn dump_cell(
    layout: &Shared<Option<Layout>>,
    root: &Shared<Option<Node>>,
    args: &[&str],
) -> Result<(), String> {
    if args.len() < 2 {
        return Err("usage: dump-cell <source> <mix>".into());
    }
    let source = parse_source(args[0])?;
    let mix = parse_mix(args[1])?;

    let layout_guard = layout.lock().unwrap();
    let l = layout_guard
        .as_ref()
        .ok_or_else(|| "no Layout yet; wait for the fullSync".to_string())?;
    let root_guard = root.lock().unwrap();
    let r = root_guard
        .as_ref()
        .ok_or_else(|| "no fullSync parsed yet".to_string())?;

    let s_idx = source.to_protocol();
    let m_idx = mix.to_protocol();
    let per_source = l.mix_count_per_source() as u32;
    let target = s_idx as u32 * per_source + m_idx as u32;

    let mut counter: u32 = 0;
    for child in &r.children {
        if child.name != "MIX" {
            continue;
        }
        if counter == target {
            println!("[cell] source={source:?} mix={mix:?} root_idx_offset={counter}");
            for p in &child.properties {
                println!("  {} = {:?}", p.name, p.value);
            }
            return Ok(());
        }
        counter += 1;
    }
    Err(format!(
        "cell not found (offset {target}, found {counter} MIX nodes)"
    ))
}

fn print_layout(layout: &Shared<Option<Layout>>) {
    match layout.lock().unwrap().as_ref() {
        None => println!("no layout yet"),
        Some(l) => {
            println!("model:               {}", model_label(l.model()));
            println!("fader count:         {}", l.fader_count());
            println!("channel first:       {}", l.first_channel());
            println!("channel count:       {}", l.channel_count());
            println!("source count:        {}", l.source_count());
            println!("mix per source:      {}", l.mix_count_per_source());
            println!("master channel:      {:?}", l.master_channel());
            println!("output:              {:?}", l.output());
            println!("ducker:              {:?}", l.ducker());
            println!("recorder:            {:?}", l.recorder());
            println!("player:              {:?}", l.player());
            println!(
                "headphones:          first={:?} count={}",
                l.first_headphone(),
                l.headphone_count()
            );
            println!(
                "effects:             first={:?} count={}",
                l.first_effects(),
                l.effects_count()
            );
            println!("gui:                 {:?}", l.gui());
            println!("soundpads:           {:?}", l.soundpads());
            println!(
                "pads:                first={} count={}",
                l.first_pad(),
                l.pad_count()
            );
        }
    }
}

fn print_help() {
    println!(
        "\
commands:
  help                                       show this
  quit | exit                                disconnect

  layout                                     print discovered Layout
  watch <on|off>                             toggle live event printing

  mute     <fader>            <on|off>       SetFaderMute
  cue      <fader>            <on|off>       SetFaderCue
  level    <fader>            <0..127>       SetFaderLevel (virtual faders)
  assign   <fader>            <source|clear> AssignFaderSource

  mix-disable <source> <mix>  <on|off>       SetMixDisabled
  mix-mute    <source> <mix>  <on|off>       SetMixMute
  link        <source> <mix>                 LinkMix    (enable+unmute+pulse)
  unlink      <source> <mix>                 UnlinkMix
  link-callme   <callme1..3> <mix>           LinkCallMe (single legacy blob)
  unlink-callme <callme1..3> <mix>           UnlinkCallMe

  screen                                     ScreenTouched (wake display)
  power                                      PowerOff (asks for 'yes')

  set master      <name> <kind> <value>      SetMasterParam
  set output      <name> <kind> <value>      SetOutputParam
  set ducker      <name> <kind> <value>      SetDuckerParam
  set recorder    <name> <kind> <value>      SetRecorderParam
  set player      <name> <kind> <value>      SetPlayerParam
  set gui         <name> <kind> <value>      SetGuiParam
  set system      <name> <kind> <value>      SetSystemParam
  set channel     <fader>  <name> <kind> <v> SetChannelParam
  set inputsource <source> <name> <kind> <v> SetInputSourceParam
  set headphone   <0..N>   <name> <kind> <v> SetHeadphoneParam
  set effects     <0..N>   <name> <kind> <v> SetEffectsParam
  set pad         <0..N>   <name> <kind> <v> SetPadParam

specs:
  fader  : p1..p6 / v1..v5
  source : combo1..4, combo1_2, combo2_3, combo3_4, usb1, chat, usb2,
           bt, pad, vgame, vmusic, va, vb, callme1..callme3
  mix    : hp1..hp4, spk, rec, bt, usb1, chat, usb2, callme1..callme3
  kind   : int <i32> | i64 <i64> | bool <on|off> | dbl <f64> | str <rest of line>"
    );
}
