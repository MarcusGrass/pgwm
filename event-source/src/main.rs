use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::SystemTime;
use x11rb::protocol::Event;
use x11rb::x11_utils::{ExtInfoProvider, ExtensionInformation, TryParse};
const CLIENT_SETUP: &[u8] = b"___CLIENT_SETUP___";
const SERVER_SETUP: &[u8] = b"___SERVER_SETUP___";
const CLIENT_MESSAGE: &[u8] = b"___CLIENT_OUTGOING___";
const SERVER_MESSAGE: &[u8] = b"___SERVER_OUTGOING___";
const PARSEABLE_MESSAGES: [&[u8]; 4] = [CLIENT_MESSAGE, SERVER_MESSAGE, CLIENT_SETUP, SERVER_SETUP];

const PROFILES: &[&str] = &["release", "optimized"];

fn main() {
    let long = "event-source/long.log";
    run(long, 1000);
}

#[allow(clippy::uninit_vec)]
fn run(path: impl AsRef<Path>, count: usize) {
    let messages = parse_log(path);
    let mut csv_out = "profile,runs,messages,run_average_nanos,run_median_nanos,latency_average_nanos,latency_median_nanos\n".to_owned();
    let _ = std::fs::remove_file("/tmp/.X11-unix/X4");
    eprintln!("Running {} messages", messages.len());
    let sock = UnixListener::bind("/tmp/.X11-unix/X4").unwrap();
    let lock = Arc::new(Mutex::new(()));
    for profile in PROFILES {
        let mut results = TotalRunResults {
            profile,
            runs: count,
            messages: 42000, // I just happen to know
            run_time_nanos: vec![],
            latency_nanos: vec![],
        };
        for i in 0..count {
            let child_lock = lock.clone();
            let guard = lock.lock().unwrap();
            // There's actually a race condition below, starting the WM before listening
            // but non-problematic atm
            let handle = start_wm(child_lock, profile);
            let (mut stream, _addr) = sock.accept().unwrap();
            let use_msgs = messages.clone();
            let ops = merge_messages(use_msgs);
            eprintln!("Starting run {} for profile {}", i, profile);
            let start = SystemTime::now();
            let mut latency_timer = None;
            for op in ops.into_iter().take(2199) {
                match op {
                    SockOp::Read(n) => {
                        let mut buf = Vec::with_capacity(n);
                        unsafe { buf.set_len(n) };
                        stream.read_exact(&mut buf).unwrap();
                        if let Some(lt) = latency_timer {
                            results
                                .latency_nanos
                                .push(SystemTime::now().duration_since(lt).unwrap().as_nanos());
                        }
                    }
                    SockOp::Write(buf) => {
                        stream.write_all(&buf).unwrap();
                        latency_timer = Some(SystemTime::now());
                    }
                }
            }
            let end = SystemTime::now().duration_since(start).unwrap().as_nanos();
            results.run_time_nanos.push(end);
            drop(guard);
            handle.join().unwrap().unwrap();
        }
        eprintln!("{}", results.format());
        csv_out.push_str(&format!("{}\n", results.format_csv()));
    }
    std::fs::write(
        "/home/gramar/code/rust/pgwm/target/out.csv",
        csv_out.as_bytes(),
    )
    .unwrap();
}

struct TotalRunResults {
    profile: &'static str,
    runs: usize,
    messages: usize,
    run_time_nanos: Vec<u128>,
    latency_nanos: Vec<u128>,
}

impl TotalRunResults {
    fn format(&self) -> String {
        let (run_avg, run_median) = calc_avg_median(&self.run_time_nanos);
        let (lat_avg, lat_median) = calc_avg_median(&self.latency_nanos);
        let msgs_per_sec = self.messages as f64 / run_avg as f64 * 1_000_000_000f64;
        format!(
            "----\nProfile {}, {} runs, {} messages per run\n\tThroughput:\n\
        \t\tAverage run time: {} millis.\n\
        \t\tMedian run time: {} millis.\n\
        \t\tAverage messages per second: {}\n\
        \tLatency:\n\
        \t\tAverage latency: {} millis\n\
        \t\tMedian latency: {} millis\n\
        ",
            self.profile,
            self.runs,
            self.messages,
            run_avg as f64 / 1_000_000f64,
            run_median as f64 / 1_000_000f64,
            msgs_per_sec,
            lat_avg as f64 / 1_000_000f64,
            lat_median as f64 / 1_000_000f64
        )
    }
    fn format_csv(&self) -> String {
        let (run_avg, run_median) = calc_avg_median(&self.run_time_nanos);
        let (lat_avg, lat_median) = calc_avg_median(&self.latency_nanos);
        format!(
            "{},{},{},{run_avg},{run_median},{lat_avg},{lat_median}",
            self.profile, self.runs, self.messages
        )
    }
}

fn start_wm(mutex: Arc<Mutex<()>>, profile: &'static str) -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let mut out = std::process::Command::new("cargo")
            .arg("run")
            //.arg("--target")
            //.arg("x86_64-unknown-linux-musl")
            .arg(format!("--profile={profile}"))
            .arg("--no-default-features")
            .arg("--features")
            .arg("xinerama,config-file")
            .arg("-p")
            .arg("pgwm")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let _g = mutex.lock().unwrap();
        out.kill()
    })
}

fn calc_avg_median(res: &[u128]) -> (u128, u128) {
    let sum = res.iter().sum::<u128>();
    let avg = sum / res.len() as u128;
    let median_ind = res.len() / 2;
    (avg, res[median_ind])
}

// Used for "chunking" basically if we're doing repeated reads/write we just merge them
enum SockOp {
    Read(usize),
    Write(Vec<u8>),
}

fn merge_messages(messages: Vec<PassedMessage>) -> Vec<SockOp> {
    let mut reading = false;
    let mut cur_read = 0;
    let mut cur_buf = vec![];
    let mut ops = vec![];
    for message in messages {
        match message {
            PassedMessage::ClientSetup(n) => {
                if reading {
                    cur_read += n;
                } else {
                    ops.push(SockOp::Write(std::mem::take(&mut cur_buf)));
                    cur_read += n;
                    reading = true;
                }
            }
            PassedMessage::ServerSetup(buf) => {
                if reading {
                    ops.push(SockOp::Read(std::mem::take(&mut cur_read)));
                    cur_buf.extend_from_slice(&buf);
                    reading = false;
                } else {
                    cur_buf.extend_from_slice(&buf);
                }
            }
            PassedMessage::ClientMessage(n) => {
                if reading {
                    cur_read += n;
                } else {
                    ops.push(SockOp::Write(std::mem::take(&mut cur_buf)));
                    cur_read += n;
                    reading = true;
                }
            }
            PassedMessage::ServerMessage(buf) => {
                if reading {
                    ops.push(SockOp::Read(std::mem::take(&mut cur_read)));
                    cur_buf.extend_from_slice(&buf);
                    reading = false;
                } else {
                    cur_buf.extend_from_slice(&buf);
                }
            }
        }
    }
    if reading {
        ops.push(SockOp::Read(std::mem::take(&mut cur_read)));
    } else {
        ops.push(SockOp::Write(std::mem::take(&mut cur_buf)));
    }
    ops
}

fn parse_log(path: impl AsRef<Path>) -> Vec<PassedMessage> {
    let all_bytes = std::fs::read(path).unwrap();
    assert!(all_bytes.starts_with(CLIENT_SETUP));
    let mut cursor = 0;
    let mut messages = Vec::new();
    while cursor < all_bytes.len() {
        let (new_ind, msg) = parse_next(cursor, &all_bytes);
        messages.push(msg);
        cursor = new_ind;
    }
    messages
}

// This is extremely inefficient but it doesn't really matter
fn parse_next(cursor: usize, buf: &[u8]) -> (usize, PassedMessage) {
    let mut found_delimiter = None;
    // Assuming we're getting a cursor at a delimiter
    for msg_kind in PARSEABLE_MESSAGES {
        if &buf[cursor..cursor + msg_kind.len()] == msg_kind {
            found_delimiter = Some(msg_kind);
            break;
        }
    }
    let delimiter = if let Some(fd) = found_delimiter {
        fd
    } else {
        panic!("Couldn't find delimiter for next message");
    };
    // Get the kind
    let kind = MsgKind::from_delimiter(delimiter);

    // Start read bytes after delimiter
    let start = cursor + delimiter.len();
    let mut i = start;
    loop {
        // Need to find next delimiter to know how long the message is
        let check_for = PARSEABLE_MESSAGES
            .iter()
            .filter(|msg_delimiter| buf.len() > i + msg_delimiter.len())
            .copied()
            .collect::<Vec<&[u8]>>();
        // No next delimiter, we should be at the last message
        if check_for.is_empty() {
            return (buf.len(), to_msg(kind, &buf[start..]));
        }
        // Check for all delimiters that could fit in the remaining buffer space
        for msg_delimiter in check_for {
            if &buf[i..i + msg_delimiter.len()] == msg_delimiter {
                // Found one, read up until that one
                return (i, to_msg(kind, &buf[start..i]));
            }
        }
        // Nothing found, check one index further in (this is extremely inefficient)
        i += 1;
    }
}

fn to_msg(kind: MsgKind, payload: &[u8]) -> PassedMessage {
    match kind {
        MsgKind::ClientSetup => {
            let (client_setup_evt, _) =
                x11rb::protocol::xproto::SetupRequest::try_parse(payload).unwrap();
            PassedMessage::ClientSetup(payload.len())
        }
        MsgKind::ServerSetup => {
            let (server_setup_evt, _) = x11rb::protocol::xproto::Setup::try_parse(payload).unwrap();
            PassedMessage::ServerSetup(payload.to_vec())
        }
        MsgKind::Client => PassedMessage::ClientMessage(payload.len()),
        MsgKind::Server => {
            let evt = Event::parse(payload, &HardCodedRequestInfoProvider).unwrap();
            PassedMessage::ServerMessage(payload.to_vec())
        }
    }
}

enum MsgKind {
    ClientSetup,
    ServerSetup,
    Client,
    Server,
}
impl MsgKind {
    fn from_delimiter(delimiter: &[u8]) -> MsgKind {
        match delimiter {
            CLIENT_MESSAGE => Self::Client,
            CLIENT_SETUP => Self::ClientSetup,
            SERVER_MESSAGE => Self::Server,
            SERVER_SETUP => Self::ServerSetup,
            &_ => panic!("Unrecognized delimiter {:?}", delimiter),
        }
    }
}

#[derive(Debug, Clone)]
enum PassedMessage {
    ClientSetup(usize),
    ServerSetup(Vec<u8>),
    ClientMessage(usize),
    ServerMessage(Vec<u8>),
}

#[derive(Copy, Clone)]
struct Extension {
    name: &'static str,
    info: ExtensionInformation,
}

const EXTENSIONS: [Extension; 3] = [
    Extension {
        name: "XINERAMA",
        info: ExtensionInformation {
            major_opcode: 141,
            first_event: 0,
            first_error: 0,
        },
    },
    Extension {
        name: "BIG-REQUESTS",
        info: ExtensionInformation {
            major_opcode: 133,
            first_event: 0,
            first_error: 0,
        },
    },
    Extension {
        name: "RENDER",
        info: ExtensionInformation {
            major_opcode: 139,
            first_event: 0,
            first_error: 142,
        },
    },
];

struct HardCodedRequestInfoProvider;
// server (41): QueryExtension(QueryExtensionReply { sequence: 41, length: 0, present: true, major_opcode: 133, first_event: 0, first_error: 0 })
// server (35): QueryExtension(QueryExtensionReply { sequence: 35, length: 0, present: true, major_opcode: 139, first_event: 0, first_error: 142 })
// server (76): QueryExtension(QueryExtensionReply { sequence: 76, length: 0, present: true, major_opcode: 141, first_event: 0, first_error: 0 })

impl ExtInfoProvider for HardCodedRequestInfoProvider {
    fn get_from_major_opcode(&self, major_opcode: u8) -> Option<(&str, ExtensionInformation)> {
        EXTENSIONS
            .iter()
            .find_map(|ext| (ext.info.major_opcode == major_opcode).then(|| (ext.name, ext.info)))
    }

    fn get_from_event_code(&self, event_code: u8) -> Option<(&str, ExtensionInformation)> {
        EXTENSIONS
            .iter()
            .filter_map(|ext| (ext.info.first_event <= event_code).then(|| (ext.name, ext.info)))
            .max_by_key(|a| a.1.first_event)
    }

    fn get_from_error_code(&self, error_code: u8) -> Option<(&str, ExtensionInformation)> {
        EXTENSIONS
            .iter()
            .filter_map(|ext| (ext.info.first_error <= error_code).then(|| (ext.name, ext.info)))
            .max_by_key(|a| a.1.first_error)
    }
}
