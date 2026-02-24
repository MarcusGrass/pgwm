#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use heapless::spsc::{Consumer, Queue};
use tiny_std::{eprintln, unix_lit};
use tiny_std::io::Write;
use tiny_std::net::UnixStream;
use tiny_std::time::Instant;

enum QueueMetric {
    Static {
        name: &'static str,
        labels: &'static [(&'static str, &'static str)],
        value: f64,
    }
}
const QUEUE_CAP: usize = 128;
static GLOBAL_METRICS_QUEUE: tiny_std::sync::Mutex<Option<heapless::spsc::Producer<QueueMetric, QUEUE_CAP>>> = tiny_std::sync::Mutex::new(None);

static GLOBAL_QUEUE_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init_queue() {
    if GLOBAL_QUEUE_INITIALIZED.load(Ordering::Acquire) {
        panic!("Global queue initialized twice");
    }
    let queue: *mut Queue<QueueMetric, QUEUE_CAP> = {
        static mut Q: Queue<QueueMetric, QUEUE_CAP> = Queue::new();
        // SAFETY: `Q` is only accessible in this scope
        // and `main` is only called once.
        &raw mut Q
    };
    let (producer, consumer)= unsafe {queue.as_mut().unwrap()}.split();
    tiny_std::thread::spawn(move || {
        sender_worker(consumer)
    }).unwrap();
    *GLOBAL_METRICS_QUEUE.lock() = Some(producer);
}

pub struct TimedMetricsGuard {
    name: &'static str,
    labels: &'static [(&'static str, &'static str)],
    start: Instant,
}

impl TimedMetricsGuard {
    pub fn new_unlabeled(name: &'static str) -> Self {
        Self { name, labels: &[], start: Instant::now() }
    }

    pub fn new_labeled(name: &'static str, labels: &'static [(&'static str, &'static str)]) -> Self {
        Self { name, labels, start: Instant::now() }
    }
}

impl Drop for TimedMetricsGuard {
    fn drop(&mut self) {
        let time = self.start.elapsed().unwrap();
        handoff_gauge(self.name, self.labels, time.as_secs_f64());
    }
}



pub fn handoff_gauge(name: &'static str, labels: &'static [(&'static str, &'static str)], value: f64) {
    let mut queue = GLOBAL_METRICS_QUEUE.lock();
    if let Some(q) = queue.as_mut() {
       if q.enqueue(QueueMetric::Static { name, labels, value }).is_err() {
           eprintln!("Global queue enqueue failed");
       };
    }
}

fn sender_worker(mut consumer: Consumer<'static, QueueMetric, QUEUE_CAP>) {
    let mut socket = None;
    loop {
        let mut outbound = alloc::vec::Vec::new();
        match consumer.dequeue() {
            Some(QueueMetric::Static { name, labels, value }) => {
                match gauge_set_to_wire(name, labels, value) {
                    Ok(data) => {
                        outbound.extend_from_slice(&data);
                    }
                    Err(e) => {
                        eprintln!("Global queue enqueue failed: {}", e);
                    }
                };
            }
            None => {
                if outbound.len() > 0 {
                    if let Some(s) = socket.as_mut() {
                        outbound.clear();
                    } else {
                        socket = try_socket();
                    }
                }
                tiny_std::thread::sleep(Duration::from_millis(10))
                    .unwrap();
            }
        }
    }
}

fn try_socket() -> Option<UnixStream> {
    match UnixStream::connect(unix_lit!("/tmp/pgwm-metrics.socket")) {
        Ok(s) => {
            Some(s)
        }
        Err(e) => {
            eprintln!("failed to connect to metrics socket at /tmp/pgwm-metrics.socket: {}", e);
            None
        }
    }
}

fn try_send_gauge(stream: &mut UnixStream, wire: &[u8]) -> bool {
    match stream.write_all(wire) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("failed to write metric to socket: {e}");
            false
        }
    }
}

fn gauge_set_to_wire(name: &'static str, labels: &[(&'static str, &'static str)], value: f64) -> core::result::Result<alloc::vec::Vec<u8>, alloc::string::String> {
    let mut bytes = alloc::vec![0, 0, 0, 0];
    let name_len: u8 = name.len()
        .try_into()
        .map_err(|_e| "metrics name too long (max 255 bytes)")?;
    bytes.push(name_len);
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&value.to_le_bytes());
    let labels_len: u8 = labels.len()
        .try_into()
        .map_err(|_e| "metrics has too many labels".to_string())?;
    bytes.push(labels_len);
    for (label, value) in labels {
        let name_len : u8= label.len().try_into().map_err(|_e| "metrics label name too long".to_string())?;
        bytes.push(name_len);
        bytes.extend_from_slice(label.as_bytes());
        let value_len: u8 = value.len().try_into().map_err(|_e| "metrics value too long".to_string())?;
        bytes.push(value_len);
        bytes.extend_from_slice(value.as_bytes());
    }
    let payload_len = bytes.len() - 4;
    let payload_len: u32 = payload_len.try_into()
        .map_err(|_e| "metrics payload too large".to_string())?;
    bytes[..4].copy_from_slice(&payload_len.to_le_bytes());
    Ok(bytes)
}
