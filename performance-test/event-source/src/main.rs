use event_lib::{parse_messages, MessageKind, ReconstructedMessage};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Stdio;
use std::thread::JoinHandle;
use std::time::SystemTime;
use x11rb::x11_utils::{ExtInfoProvider, ExtensionInformation};

const PROFILES: [Profile; 2] = [Profile::Release, Profile::Optimized];

#[derive(Copy, Clone, Debug)]
enum Profile {
    Release,
    Optimized,
}

const STARTUP_SCENARIO: Scenario = Scenario {
    name: "Start WM",
    file: "performance-test/event-source/startup-scenario.log",
    last_client_msg_pre_startup: 621,
};

const LONG_RUN_SCENARIO: Scenario = Scenario {
    name: "Run long",
    file: "performance-test/event-source/long-run-scenario.log",
    last_client_msg_pre_startup: 652,
};

struct Scenario {
    name: &'static str,
    file: &'static str,
    last_client_msg_pre_startup: usize,
}

fn main() {
    run_scenario(LONG_RUN_SCENARIO, 10);
}

fn run_scenario(scenario: Scenario, count: usize) {
    let raw = std::fs::read(scenario.file).unwrap();
    let messages = parse_messages(&raw);
    let mut cl_msgs = 0;
    let mut startup_checkpoint = None;
    for (ind, msg) in messages.iter().enumerate() {
        if msg.metadata.kind == MessageKind::ClientSetup
            || msg.metadata.kind == MessageKind::ClientMessage
        {
            cl_msgs += 1;
            if cl_msgs == scenario.last_client_msg_pre_startup {
                startup_checkpoint = Some(ind);
            }
        }
    }
    let startup = &messages.as_slice()[..startup_checkpoint.unwrap() + 1];
    let merged_startup = merge_messages(startup);
    let post_start = &messages.as_slice()[startup_checkpoint.unwrap() + 1..];
    let merged_post = merge_messages(post_start);
    let mut csv_out = "profile,runs,messages,run_average_nanos,run_median_nanos,latency_average_nanos,latency_median_nanos\n".to_owned();
    let _ = std::fs::remove_file("/tmp/.X11-unix/X4");
    eprintln!("Running {} messages", messages.len());
    let sock = UnixListener::bind("/tmp/.X11-unix/X4").unwrap();
    let mut startup_results = vec![];
    let mut post_results = vec![];
    for profile in PROFILES {
        let binary = produce_binary(profile).unwrap();
        for i in 0..count {
            // There's actually a race condition below, starting the WM before listening
            // but non-problematic atm
            let handle = start_wm(binary.clone());
            let (mut stream, _addr) = sock.accept().unwrap();
            let startup_result = time_chunk(startup, &merged_startup, &mut stream);
            startup_results.push(startup_result);
            let post_result = time_chunk(post_start, &merged_post, &mut stream);
            post_results.push(post_result);
            handle.join().unwrap().unwrap();
            //eprintln!("Completed pass {i} for profile {profile:?}");
        }
        eprintln!(
            "{}",
            fmt_results(
                profile,
                std::mem::take(&mut startup_results),
                std::mem::take(&mut post_results),
                messages.len()
            )
        );
    }
}

#[allow(clippy::uninit_vec)]
fn time_chunk(
    messages: &[ReconstructedMessage],
    ops: &[SockOp],
    stream: &mut UnixStream,
) -> RunResult {
    let mut latency_timer = None;
    let mut latency = vec![];
    let start = SystemTime::now();
    for op in ops {
        match op {
            SockOp::Read(n) => {
                let mut buf = Vec::with_capacity(*n);
                unsafe { buf.set_len(*n) };
                stream.read_exact(&mut buf).unwrap();
                if let Some(lt) = latency_timer {
                    latency.push(SystemTime::now().duration_since(lt).unwrap().as_nanos());
                }
            }
            SockOp::Write(buf) => {
                stream.write_all(&buf).unwrap();
                latency_timer = Some(SystemTime::now());
            }
        }
    }
    let run_time_nanos = SystemTime::now().duration_since(start).unwrap().as_nanos();
    let med_ind = latency.len() / 2;
    let med_latency = latency[med_ind];
    let latency_len = latency.len();
    let avg_latency = latency.into_iter().sum::<u128>() / latency_len as u128;
    let tp = messages.len() as f64 / run_time_nanos as f64 * 1_000_000_000f64;
    RunResult {
        run_time_nanos,
        avg_latency_nanos: avg_latency,
        median_latency_nanos: med_latency,
        throughput: tp,
    }
}

fn fmt_results(
    profile: Profile,
    startup: Vec<RunResult>,
    post: Vec<RunResult>,
    messages: usize,
) -> String {
    format!(
        "Finished proccessing {messages} messages for profile {profile:?}\n\
        Startup results: \n{}\nPost start results: \n{}\n",
        fmt_single(startup),
        fmt_single(post)
    )
}

fn fmt_single(run_results: Vec<RunResult>) -> String {
    let count = run_results.len();
    let mut total_run = 0;
    let mut total_tp = 0f64;
    let mut total_avg_lat = 0;
    let mut total_med_lat = 0;
    for res in run_results {
        total_run += res.run_time_nanos;
        total_tp += res.throughput;
        total_avg_lat += res.avg_latency_nanos;
        total_med_lat += res.median_latency_nanos;
    }
    format!(
        "\tAverage run time nanos:\n\
    \t\t{}\n\
    \tAverage messages per second:\n\
    \t\t{}\n\
    \tAverage latency:\n\
    \t\t{}\n\
    \tAverage median latency:\n\
    \t\t{}",
        total_run / count as u128,
        total_tp / count as f64,
        total_avg_lat / count as u128,
        total_med_lat / count as u128
    )
}

#[derive(Debug)]
struct RunResult {
    run_time_nanos: u128,
    avg_latency_nanos: u128,
    median_latency_nanos: u128,
    throughput: f64,
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
    let out = std::process::Command::new("cargo")
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

fn start_wm(binary: PathBuf) -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let out = std::process::Command::new(binary)
            //.stdout(Stdio::null())
            //.stderr(Stdio::null())
            .output()?;
        if !out.status.success() {
            panic!("Unsuccessful run")
        }
        Ok(())
    })
}

// Used for "chunking" basically if we're doing repeated reads/write we just merge them
enum SockOp {
    Read(usize),
    Write(Vec<u8>),
}

fn merge_messages(messages: &[ReconstructedMessage]) -> Vec<SockOp> {
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
