use event_lib::{parse_messages, MessageKind, ReconstructedMessage};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
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

const PROFILES: [Profile; 2] = [Profile::Release, Profile::Optimized];

#[derive(Copy, Clone, Debug)]
enum Profile {
    Release,
    Optimized,
}

const STARTUP_SCENARIO: Scenario = Scenario {
    name: "Start WM",
    file: "performance-test/event-source/ser_events1.log",
};

struct Scenario {
    name: &'static str,
    file: &'static str,
}

fn main() {
    let long = "event-source/short_scenario.log";
    //run(long, 1000);
    run_scenario(STARTUP_SCENARIO, 100);
}

#[allow(clippy::uninit_vec)]
fn run_scenario(scenario: Scenario, count: usize) {
    let raw = std::fs::read(scenario.file).unwrap();
    let messages = parse_messages(&raw);
    eprintln!("{}", messages.len());
    eprintln!("{:?}", messages[0]);
    eprintln!("{:?}", messages[1]);
    let merged = merge_messages(messages.clone());
    eprintln!("{}", merged.len());
    let mut csv_out = "profile,runs,messages,run_average_nanos,run_median_nanos,latency_average_nanos,latency_median_nanos\n".to_owned();
    let _ = std::fs::remove_file("/tmp/.X11-unix/X4");
    eprintln!("Running {} messages", messages.len());
    let sock = UnixListener::bind("/tmp/.X11-unix/X4").unwrap();
    for profile in PROFILES {
        let binary = produce_binary(profile).unwrap();
        let mut results = TotalRunResults {
            profile,
            runs: count,
            messages: messages.len(), // I just happen to know
            run_time_nanos: vec![],
            latency_nanos: vec![],
        };
        for i in 0..count {
            // There's actually a race condition below, starting the WM before listening
            // but non-problematic atm
            let handle = start_wm(binary.clone());
            let (mut stream, _addr) = sock.accept().unwrap();
            let use_msgs = messages.clone();
            let ops = merge_messages(use_msgs);
            eprintln!("Starting run {} for profile {:?}", i, profile);
            let start = SystemTime::now();
            let mut latency_timer = None;
            for op in ops.into_iter() {
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

fn produce_binary(profile: Profile) -> std::io::Result<PathBuf> {
    match profile {
        Profile::Release => {
            run_build("release")?;
            Ok(PathBuf::from("target/release/pgwm"))
        }
        Profile::Optimized => {
            run_build("optimized")?;
            Ok(PathBuf::from("target/optimized/pgwm"))
        }
    }
}

fn run_build(profile: &str) -> std::io::Result<()> {
    let mut out = std::process::Command::new("cargo")
        .arg("b")
        //.arg("--target")
        //.arg("x86_64-unknown-linux-musl")
        .arg(format!("--profile={profile}"))
        .arg("--no-default-features")
        .arg("--features")
        .arg("xinerama,config-file,perf-test")
        .arg("-p")
        .arg("pgwm")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    eprintln!("Successfully built pgwm {profile}");
    Ok(())
}

struct TotalRunResults {
    profile: Profile,
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
            "----\nProfile {:?}, {} runs, {} messages per run\n\tThroughput:\n\
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
            "{:?},{},{},{run_avg},{run_median},{lat_avg},{lat_median}",
            self.profile, self.runs, self.messages
        )
    }
}

fn start_wm(binary: PathBuf) -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let out = std::process::Command::new(binary)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()?;
        if !out.status.success() {
            panic!("Unsuccessful run")
        }
        Ok(())
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

fn merge_messages(messages: Vec<ReconstructedMessage>) -> Vec<SockOp> {
    let mut reading = false;
    let mut cur_read = 0;
    let mut cur_buf = vec![];
    let mut ops = vec![];
    for message in messages {
        match message.metadata.kind {
            MessageKind::ClientSetup | MessageKind::ClientMessage => {
                cur_read += message.payload.len();
                if !reading {
                    ops.push(SockOp::Write(std::mem::take(&mut cur_buf)));
                    reading = true;
                }
            }
            MessageKind::ServerSetup | MessageKind::ServerMessage => {
                cur_buf.extend_from_slice(&message.payload);
                if reading {
                    ops.push(SockOp::Read(std::mem::take(&mut cur_read)));
                    reading = false;
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
