use tiny_std::time::Instant;

use crate::error::Error;
use crate::status::sys::net::Data;

#[derive(Clone)]
pub struct ThroughputChecker {
    prev_data: Data,
    prev_check: Instant,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ThroughputPerSec {
    pub down: f64,
    pub up: f64,
}

impl ThroughputChecker {
    pub fn get_throughput(&mut self) -> Result<ThroughputPerSec, Error> {
        let net_stats = crate::status::sys::net::read_net_stats()?;
        let now = Instant::now();
        let time_passed = now
            .duration_since(self.prev_check)
            .map_or(1f64, |d| d.as_secs_f64());

        let in_diff = (net_stats.bytes_in - self.prev_data.bytes_in) as f64 / time_passed;
        let out_diff = (net_stats.bytes_out - self.prev_data.bytes_out) as f64 / time_passed;
        self.prev_data = net_stats;

        self.prev_check = now;
        Ok(ThroughputPerSec {
            down: in_diff,
            up: out_diff,
        })
    }

    pub fn parse_throughput(&mut self, raw_data: &[u8]) -> Result<ThroughputPerSec, Error> {
        let net_stats = crate::status::sys::net::parse_raw(raw_data)?;
        let now = Instant::now();
        let time_passed = now
            .duration_since(self.prev_check)
            .map_or(1f64, |d| d.as_secs_f64());

        let in_diff = (net_stats.bytes_in - self.prev_data.bytes_in) as f64 / time_passed;
        let out_diff = (net_stats.bytes_out - self.prev_data.bytes_out) as f64 / time_passed;
        self.prev_data = net_stats;

        self.prev_check = now;
        Ok(ThroughputPerSec {
            down: in_diff,
            up: out_diff,
        })
    }
}

impl Default for ThroughputChecker {
    fn default() -> Self {
        let throughput = Data {
            bytes_in: 0,
            bytes_out: 0,
        };
        Self {
            prev_data: throughput,
            prev_check: Instant::now(),
        }
    }
}
