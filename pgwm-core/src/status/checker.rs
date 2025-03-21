use alloc::string::ToString;
use core::cmp::Ordering;
use core::ops::Add;
use core::time::Duration;

use heapless::String;
use smallmap::{Collapse, Map};
use tiny_std::time::Instant;

use crate::config::{STATUS_CHECKS, _STATUS_BAR_CHECK_CONTENT_LIMIT};
use crate::format_heapless;
use crate::status::cpu::{read_temp, LoadChecker, RyzenTemp};
use crate::status::net::{ThroughputChecker, ThroughputPerSec};
use crate::status::sys::bat::parse_battery_percentage;
use crate::status::sys::mem::{parse_raw, Data};
use crate::status::time::ClockFormatter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Check {
    pub interval: u64,
    pub check_type: CheckType,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum CheckType {
    Battery(BatChecks),
    Cpu(CpuFormat),
    CpuTemp(CpuTempFormat),
    Net(NetFormat),
    Mem(MemFormat),
    Date(DateFormat),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct BatChecks {
    checks: &'static [BatFormat],
}

impl BatChecks {
    #[must_use]
    pub const fn new(checks: &'static [BatFormat]) -> Self {
        let mut ind = 0;
        let mut last = u8::MAX;
        while ind < checks.len() {
            if checks[ind].above > last {
                panic!("Found a battery check in ascending order, descending order necessary");
            } else if checks[ind].above == last {
                panic!("Fonud two battery checks on the same threshold");
            } else {
                last = checks[ind].above;
            }
            ind += 1;
        }
        Self { checks }
    }

    #[inline]
    #[must_use]
    pub const fn get_checks(&self) -> &'static [BatFormat] {
        self.checks
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BatFormat {
    pub(crate) above: u8,
    pub(crate) icon: &'static str,
}

impl BatFormat {
    #[must_use]
    pub fn new(above: u8, icon: &'static str) -> Self {
        Self { above, icon }
    }

    fn format_bat(&self, capacity: u8) -> Option<String<_STATUS_BAR_CHECK_CONTENT_LIMIT>> {
        if self.above <= capacity {
            Some(format_heapless!("{} {}%", self.icon, capacity))
        } else {
            None
        }
    }

    #[must_use]
    pub fn max_length_content(&self) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        format_heapless!("{} 100%", self.icon)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CpuFormat {
    icon: &'static str,
    decimals: usize,
}

impl CpuFormat {
    #[must_use]
    pub const fn new(icon: &'static str, decimals: usize) -> Self {
        Self { icon, decimals }
    }

    fn format_cpu(&self, load_percentage: f64) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let chars = if self.decimals > 0 {
            self.decimals + 4
        } else {
            3
        };
        format_heapless!(
            "{} {:N$.D$}%",
            self.icon,
            load_percentage,
            N = chars,
            D = self.decimals
        )
    }

    #[must_use]
    pub fn max_length_content(&self) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let chars = if self.decimals > 0 {
            self.decimals + 4
        } else {
            3
        };
        format_heapless!(
            "{} {:N$.D$}%",
            self.icon,
            99.999_999_999,
            N = chars,
            D = self.decimals
        )
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CpuTempFormat {
    icon: &'static str,
}

impl CpuTempFormat {
    #[must_use]
    pub const fn new(icon: &'static str) -> Self {
        Self { icon }
    }

    fn format_temp(&self, temp: RyzenTemp) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        format_heapless!(
            "{} {:3.1}c, {:3.1}c, {:3.1}c",
            self.icon,
            temp.tctl,
            temp.die1,
            temp.die2,
        )
    }

    #[must_use]
    pub fn max_length_content(&self) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        format_heapless!(
            "{} {:3.1}c, {:3.1}c, {:3.1}c",
            self.icon,
            999.9,
            999.9,
            999.9
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NetFormat {
    icon_up: &'static str,
    icon_down: &'static str,
    decimals: usize,
}

impl NetFormat {
    #[must_use]
    pub const fn new(icon_up: &'static str, icon_down: &'static str, decimals: usize) -> Self {
        Self {
            icon_up,
            icon_down,
            decimals,
        }
    }
    fn format_net(&self, net_stats: ThroughputPerSec) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let (up_short, up_val) = compress_to_display(net_stats.up);
        let chars = if self.decimals > 0 {
            self.decimals + 4
        } else {
            3
        };
        let (dn_short, dn_val) = compress_to_display(net_stats.down);
        format_heapless!(
            "{} {:N$.D$}{} {} {:N$.D$}{}",
            self.icon_down,
            dn_val,
            dn_short,
            self.icon_up,
            up_val,
            up_short,
            N = chars,
            D = self.decimals
        )
    }

    #[must_use]
    pub fn max_length_content(&self) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let chars = if self.decimals > 0 {
            self.decimals + 4
        } else {
            3
        };
        format_heapless!(
            "{} {:N$.D$}{} {} {:N$.D$}{}",
            self.icon_down,
            999.999_999_999f64,
            "GB",
            self.icon_up,
            999.999_999_999f64,
            "GB",
            N = chars,
            D = self.decimals
        )
    }
}

// Just...
fn compress_to_display(val: f64) -> (&'static str, f64) {
    if val < 1_000_000f64 {
        ("KB", val / 1000f64)
    } else if val < 1_000_000_000f64 {
        ("MB", val / 1_000_000f64)
    } else if val < 1_000_000_000_000f64 {
        ("GB", val / 1_000_000_000f64)
    } else if val < 1_000_000_000_000_000f64 {
        ("TB", val / 1_000_000_000_000f64)
    } else {
        ("PB", val / 1_000_000_000_000_000f64)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemFormat {
    icon: &'static str,
    decimals: usize,
}

impl MemFormat {
    #[must_use]
    pub const fn new(icon: &'static str, decimals: usize) -> Self {
        Self { icon, decimals }
    }

    fn format_mem(&self, mem_info: Data) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let chars = if self.decimals > 0 {
            4 + self.decimals
        } else {
            3
        };
        let (mem_unit, mem_val) = compress_to_display(mem_info.available as f64 * 1000f64);
        format_heapless!(
            "{} {:N$.D$}{}",
            self.icon,
            mem_val,
            mem_unit,
            N = chars,
            D = self.decimals,
        )
    }

    #[must_use]
    pub fn max_length_content(&self) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let chars = if self.decimals > 0 {
            self.decimals + 4
        } else {
            3
        };
        format_heapless!(
            "{} {:N$.D$}{}",
            self.icon,
            999.999_999_999f64,
            "GB",
            N = chars,
            D = self.decimals,
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DateFormat {
    icon: &'static str,
    clock_formatter: ClockFormatter,
}

impl DateFormat {
    #[must_use]
    pub const fn new(icon: &'static str, clock_formatter: ClockFormatter) -> Self {
        Self {
            icon,
            clock_formatter,
        }
    }

    #[must_use]
    pub fn format_date(&self) -> String<_STATUS_BAR_CHECK_CONTENT_LIMIT> {
        let output = self
            .clock_formatter
            .format_now()
            .unwrap_or_else(|_| "Failed to format get date".to_string());
        format_heapless!("{} {}", self.icon, output)
    }
}

pub struct Checker<'a> {
    cpu_checker: LoadChecker,
    net_checker: ThroughputChecker,
    checks_by_key: Map<NextCheck, PackagedCheck<'a>>,
}

#[derive(PartialEq, Eq)]
struct PackagedCheck<'a> {
    next_time: Instant,
    check: &'a Check,
    position: usize,
}

impl PackagedCheck<'_> {
    fn update_check_time(&mut self) {
        // Using this instead of SystemTime now avoids de-syncs between checks and unnecessary system calls
        self.next_time = self
            .next_time
            .add(Duration::from_millis(self.check.interval))
            .unwrap();
    }
}

impl PartialOrd<Self> for PackagedCheck<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PackagedCheck<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.next_time.cmp(&other.next_time)
    }
}

pub struct CheckResult {
    pub content: Option<String<_STATUS_BAR_CHECK_CONTENT_LIMIT>>,
    pub position: usize,
    pub next_check: Instant,
}

pub struct CheckSubmitAdvice {
    pub submit_indices: heapless::FnvIndexSet<NextCheck, { STATUS_CHECKS.len() }>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum NextCheck {
    BAT = 0,
    CPU = 1,
    NET = 2,
    MEM = 3,
    Date = 4,
    CpuTemp = 5,
}

impl Collapse for NextCheck {
    fn collapse(&self) -> u8 {
        *self as u8
    }
}

impl<'a> Checker<'a> {
    pub fn get_all_check_submits(
        &mut self,
    ) -> heapless::Vec<(NextCheck, Instant), { STATUS_CHECKS.len() }> {
        let mut all = heapless::Vec::new();
        for (next, check) in self.checks_by_key.iter_mut() {
            let _ = all.push((*next, check.next_time));
        }
        all
    }
    pub fn handle_completed(
        &mut self,
        completed: NextCheck,
        content: &[u8],
    ) -> Option<CheckResult> {
        let packaged = self.checks_by_key.get_mut(&completed)?;
        let content = match &packaged.check.check_type {
            CheckType::Battery(limits) => parse_battery_percentage(content).ok().and_then(|bat| {
                limits
                    .get_checks()
                    .iter()
                    .find_map(|limit| limit.format_bat(bat))
            }),
            CheckType::Cpu(fmt) => self
                .cpu_checker
                .parse_load(content)
                .ok()
                .map(|cpu| fmt.format_cpu(cpu)),
            CheckType::CpuTemp(fmt) => read_temp().ok().map(|rt| fmt.format_temp(rt)),
            CheckType::Net(fmt) => self
                .net_checker
                .parse_throughput(content)
                .ok()
                .map(|tp| fmt.format_net(tp)),
            CheckType::Mem(fmt) => parse_raw(content).ok().map(|mem| fmt.format_mem(mem)),
            CheckType::Date(fmt) => Some(fmt.format_date()),
        };
        packaged.update_check_time();
        Some(CheckResult {
            content,
            position: packaged.position,
            next_check: packaged.next_time,
        })
    }

    pub fn new(checks: &'a mut [Check]) -> Self {
        let mut checks_by_key = Map::new();
        let sync_start_time = Instant::now();
        for (position, check) in checks.iter().enumerate() {
            match check.check_type {
                CheckType::Battery(_) => {
                    checks_by_key.insert(
                        NextCheck::BAT,
                        PackagedCheck {
                            next_time: sync_start_time,
                            check,
                            position,
                        },
                    );
                }
                CheckType::Cpu(_) => {
                    checks_by_key.insert(
                        NextCheck::CPU,
                        PackagedCheck {
                            next_time: sync_start_time,
                            check,
                            position,
                        },
                    );
                }
                CheckType::CpuTemp(_) => {
                    checks_by_key.insert(
                        NextCheck::CpuTemp,
                        PackagedCheck {
                            next_time: sync_start_time,
                            check,
                            position,
                        },
                    );
                }
                CheckType::Net(_) => {
                    checks_by_key.insert(
                        NextCheck::NET,
                        PackagedCheck {
                            next_time: sync_start_time,
                            check,
                            position,
                        },
                    );
                }
                CheckType::Mem(_) => {
                    checks_by_key.insert(
                        NextCheck::MEM,
                        PackagedCheck {
                            next_time: sync_start_time,
                            check,
                            position,
                        },
                    );
                }
                CheckType::Date(_) => {
                    checks_by_key.insert(
                        NextCheck::Date,
                        PackagedCheck {
                            next_time: sync_start_time,
                            check,
                            position,
                        },
                    );
                }
            }
        }

        Checker {
            cpu_checker: LoadChecker::default(),
            net_checker: ThroughputChecker::default(),
            checks_by_key,
        }
    }
}

#[cfg(test)]
mod checker_tests {
    use core::ops::Add;
    use core::time::Duration;

    use tiny_std::time::Instant;

    use crate::status::checker::{Check, CheckType, Checker, CpuFormat};

    #[test]
    #[cfg(unix)]
    fn can_real_run_checks() {
        let mut checks: heapless::Vec<Check, 1> = heapless::Vec::new();

        let interval = Duration::from_millis(10_000);
        let _ = checks.push(Check {
            interval: interval.as_millis() as u64,
            check_type: CheckType::Cpu(CpuFormat {
                icon: "hello",
                decimals: 2,
            }),
        });
        let now = Instant::now();
        let mut checker = Checker::new(&mut checks);
        let mut run_checks = checker.get_all_check_submits();
        assert_eq!(1, run_checks.len());
        let (_next, when) = run_checks.pop().unwrap();
        // If this test takes more than 10 seconds there are other issues
        assert!(when < now.add(2 * interval).unwrap());
    }
}
