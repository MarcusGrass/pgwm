use crate::error::Error;
use crate::status::sys::cpu::Load;

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
