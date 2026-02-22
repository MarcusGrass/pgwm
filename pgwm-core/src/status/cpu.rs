use crate::error::Error;
use crate::status::sys::cpu::Load;
use tiny_std::{UnixStr, unix_lit};

#[derive(Default)]
pub struct LoadChecker {
    prev_load: Load,
}

impl LoadChecker {
    pub fn get_load(&mut self) -> Result<f64, Error> {
        let cur_ld = crate::status::sys::cpu::read_cpu_load()?;
        let ld = &self.prev_load;
        let load_perc = calculate_load(ld, &cur_ld);
        self.prev_load = cur_ld;
        Ok(load_perc * 100f64)
    }

    #[inline]
    pub fn parse_load(&mut self, buf: &[u8]) -> Result<f64, Error> {
        let cur_ld = crate::status::sys::cpu::parse_raw(buf)?;
        let ld = &self.prev_load;
        let load_perc = calculate_load(ld, &cur_ld);
        self.prev_load = cur_ld;
        Ok(load_perc * 100f64)
    }
}

fn calculate_load(prev: &Load, cur: &Load) -> f64 {
    let prev_total = prev.idle + prev.busy;
    let cur_total = cur.idle + cur.busy;
    let total_diff = cur_total - prev_total;
    let idle_diff = cur.idle - prev.idle;
    if idle_diff == 0f64 || total_diff == 0f64 {
        return 1f64;
    }
    1f64 - idle_diff / total_diff
}

#[derive(Debug)]
pub struct RyzenTemp {
    pub tctl: f32,
    pub die1: f32,
    pub die2: f32,
}
pub fn read_temp() -> tiny_std::Result<RyzenTemp> {
    const BASE: &UnixStr = unix_lit!("/sys/devices/pci0000:00/0000:00:18.3/hwmon/hwmon4");
    let mut tctl = 0.0;
    let mut die1 = 0.0;
    let mut die2 = 0.0;
    for i in [1, 3, 4] {
        let label_path = BASE.path_join_fmt(format_args!("temp{i}_label\0"));
        let mut raw_label = tiny_std::fs::read(&label_path)?;
        // Bump newline
        raw_label.pop();
        let label_path = BASE.path_join_fmt(format_args!("temp{i}_input\0"));
        let raw_temp = tiny_std::fs::read_to_string(&label_path)?;
        let rt_i32: i32 = raw_temp.trim().parse().unwrap();
        let temp = rt_i32 as f32 / 1000.0;
        match raw_label.as_slice() {
            b"Tctl" => tctl = temp,
            b"Tccd1" => die1 = temp,
            b"Tccd2" => die2 = temp,
            &_ => {}
        }
    }
    Ok(RyzenTemp { tctl, die1, die2 })
}
