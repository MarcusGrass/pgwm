use alloc::string::ToString;
use core::cmp::Ordering;
use core::ops::Add;
use core::time::Duration;

use heapless::binary_heap::Min;
use heapless::{BinaryHeap, String};
use tiny_std::time::Instant;

use crate::config::{
    STATUS_BAR_BAT_SEGMENT_LIMIT, STATUS_BAR_CHECK_CONTENT_LIMIT, STATUS_BAR_UNIQUE_CHECK_LIMIT,
};
use crate::format_heapless;
use crate::status::cpu::LoadChecker;
use crate::status::net::{ThroughputChecker, ThroughputPerSec};
use crate::status::sys::bat::BatChecker;
use crate::status::sys::mem::{Data, MemChecker};
use crate::status::time::ClockFormatter;

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Check {
    pub interval: u64,
    pub check_type: CheckType,
}

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(feature = "config-file", serde(tag = "kind", content = "args"))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CheckType {
    Battery(heapless::Vec<BatFormat, STATUS_BAR_BAT_SEGMENT_LIMIT>),
    Cpu(CpuFormat),
    Net(NetFormat),
    Mem(MemFormat),
    Date(DateFormat),
}

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BatFormat {
    pub(crate) above: u8,
    pub(crate) icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
}

impl BatFormat {
    #[must_use]
    pub fn new(above: u8, icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>) -> Self {
        Self { above, icon }
    }

    fn format_bat(&self, capacity: u8) -> Option<String<STATUS_BAR_CHECK_CONTENT_LIMIT>> {
        if self.above <= capacity {
            Some(format_heapless!("{} {}%", self.icon, capacity))
        } else {
            None
        }
    }

    #[must_use]
    pub fn max_length_content(&self) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
        format_heapless!("{} 100%", self.icon)
    }
}

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CpuFormat {
    icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
    decimals: usize,
}

impl CpuFormat {
    #[must_use]
    pub fn new(icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>, decimals: usize) -> Self {
        Self { icon, decimals }
    }

    fn format_cpu(&self, load_percentage: f64) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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
    pub fn max_length_content(&self) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NetFormat {
    icon_up: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
    icon_down: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
    decimals: usize,
}

impl NetFormat {
    #[must_use]
    pub fn new(
        icon_up: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        icon_down: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        decimals: usize,
    ) -> Self {
        Self {
            icon_up,
            icon_down,
            decimals,
        }
    }
    fn format_net(&self, net_stats: ThroughputPerSec) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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
    pub fn max_length_content(&self) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemFormat {
    icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
    decimals: usize,
}

impl MemFormat {
    #[must_use]
    pub fn new(icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>, decimals: usize) -> Self {
        Self { icon, decimals }
    }

    fn format_mem(&self, mem_info: Data) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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
    pub fn max_length_content(&self) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DateFormat {
    icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
    clock_formatter: ClockFormatter,
}

impl DateFormat {
    #[must_use]
    pub fn new(
        icon: String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        clock_formatter: ClockFormatter,
    ) -> Self {
        Self {
            icon,
            clock_formatter,
        }
    }

    #[must_use]
    pub fn format_date(&self) -> String<STATUS_BAR_CHECK_CONTENT_LIMIT> {
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
    mem_checker: MemChecker,
    bat_checker: BatChecker,
    check_heap: BinaryHeap<PackagedCheck<'a>, Min, STATUS_BAR_UNIQUE_CHECK_LIMIT>,
}

#[derive(PartialEq, Eq)]
struct PackagedCheck<'a> {
    next_time: Instant,
    check: &'a Check,
    position: usize,
}

impl<'a> PackagedCheck<'a> {
    fn update_check_time(&mut self) {
        // Using this instead of SystemTime now avoids de-syncs between checks and unnecessary system calls
        self.next_time = self
            .next_time
            .add(Duration::from_millis(self.check.interval))
            .unwrap();
    }
}

impl<'a> PartialOrd<Self> for PackagedCheck<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.next_time.partial_cmp(&other.next_time)
    }
}

impl<'a> Ord for PackagedCheck<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.next_time.cmp(&other.next_time)
    }
}

pub struct CheckResult {
    pub content: Option<String<STATUS_BAR_CHECK_CONTENT_LIMIT>>,
    pub position: usize,
    pub next_check: Instant,
}

impl<'a> Checker<'a> {
    pub fn run_next(&mut self, dry: bool) -> CheckResult {
        if let Some(next) = self.check_heap.peek() {
            if next.next_time <= Instant::now() {
                // Checked above for existence, and is single-threaded (could pop unchecked but the performance diff is marginal)
                let position = next.position;
                let mut next = self.check_heap.pop().unwrap();
                let content = if dry {
                    next.update_check_time();
                    // Push back before next peek in the case of a single check
                    let _ = self.check_heap.push(next);
                    None
                } else {
                    // Put it back in with a new next check time
                    let res = self.run_check(&mut next);
                    next.update_check_time();
                    let _ = self.check_heap.push(next);
                    res
                };
                CheckResult {
                    content,
                    position,
                    next_check: self.check_heap.peek().unwrap().next_time,
                }
            } else {
                CheckResult {
                    content: None,
                    position: 0,
                    next_check: self.check_heap.peek().unwrap().next_time,
                }
            }
        } else {
            panic!("Tried to run status checks without any status checks present.");
        }
    }

    fn run_check(
        &mut self,
        packaged: &mut PackagedCheck,
    ) -> Option<String<STATUS_BAR_CHECK_CONTENT_LIMIT>> {
        match &packaged.check.check_type {
            CheckType::Battery(limits) => self
                .bat_checker
                .get_battery_percentage()
                .ok()
                .and_then(|bat| limits.iter().find_map(|limit| limit.format_bat(bat))),
            CheckType::Cpu(fmt) => self
                .cpu_checker
                .get_load()
                .ok()
                .map(|cpu| fmt.format_cpu(cpu)),
            CheckType::Net(fmt) => self
                .net_checker
                .get_throughput()
                .ok()
                .map(|tp| fmt.format_net(tp)),
            CheckType::Mem(fmt) => self
                .mem_checker
                .read_mem_info()
                .ok()
                .map(|mem| fmt.format_mem(mem)),
            CheckType::Date(fmt) => Some(fmt.format_date()),
        }
    }

    pub fn new(checks: &'a mut heapless::Vec<Check, STATUS_BAR_UNIQUE_CHECK_LIMIT>) -> Self {
        assert!(!checks.is_empty(), "No checks, still tried to init");
        for check in checks.iter_mut() {
            if let CheckType::Battery(bf) = &mut check.check_type {
                bf.sort_by(|a, b| a.above.cmp(&b.above));
            }
        }
        let mut check_heap = BinaryHeap::new();
        let sync_start_time = Instant::now();

        for (position, check) in checks.iter().enumerate() {
            let _ = check_heap.push(PackagedCheck {
                next_time: sync_start_time,
                check,
                position,
            });
        }
        Checker {
            cpu_checker: LoadChecker::default(),
            net_checker: ThroughputChecker::default(),
            mem_checker: MemChecker::default(),
            bat_checker: BatChecker::default(),
            check_heap,
        }
    }
}

#[cfg(test)]
mod checker_tests {
    use core::ops::{Add, Sub};
    use core::time::Duration;
    use pgwm_utils::unix_eprintln;
    use std::collections::HashSet;
    use tiny_std::time::Instant;

    use crate::status::checker::{Check, CheckType, Checker, CpuFormat};

    #[test]
    #[should_panic]
    fn immediate_panic_on_no_checks() {
        Checker::new(&mut heapless::Vec::new());
    }

    #[test]
    fn can_dry_run_checks() {
        let mut checks = heapless::Vec::new();

        let interval = Duration::from_millis(10_000);
        let _ = checks.push(Check {
            interval: interval.as_millis() as u64,
            check_type: CheckType::Cpu(CpuFormat {
                icon: heapless::String::default(),
                decimals: 2,
            }),
        });
        let now = Instant::now();
        let mut checker = Checker::new(&mut checks);
        let result = checker.run_next(true);
        assert!(result.content.is_none());
        assert!(result.next_check >= now.add(interval).unwrap());
        // If this test takes more than 10 seconds there are other issues
        assert!(result.next_check < now.add(2 * interval).unwrap());
    }

    // Risk for flakiness
    #[test]
    fn can_dry_run_kept_in_sync() {
        let mut checks = heapless::Vec::new();

        // Need some primes with no overlap on doubling/tripling within a chosen low range
        let three = Duration::from_millis(3);
        let five = Duration::from_millis(5);
        let seven = Duration::from_millis(7);
        let _ = checks.push(Check {
            interval: three.as_millis() as u64,
            check_type: CheckType::Cpu(CpuFormat {
                icon: heapless::String::default(),
                decimals: 2,
            }),
        });
        let _ = checks.push(Check {
            interval: five.as_millis() as u64,
            check_type: CheckType::Cpu(CpuFormat {
                icon: heapless::String::default(),
                decimals: 2,
            }),
        });
        let _ = checks.push(Check {
            interval: seven.as_millis() as u64,
            check_type: CheckType::Cpu(CpuFormat {
                icon: heapless::String::default(),
                decimals: 2,
            }),
        });
        let start = Instant::now();
        // Need some end duration that's big enough to allow all checks at least one run but low enough
        // to need cause multiplication overlap
        let end = start.add(Duration::from_millis(13)).unwrap();
        let mut acquired_check_times = HashSet::new();

        let mut checker = Checker::new(&mut checks);
        let mut first_check_time = None;
        while end > Instant::now() {
            let next = checker.run_next(true).next_check;
            if first_check_time.is_none() {
                first_check_time = Some(next);
            }
            acquired_check_times.insert(next);
        }
        assert!(first_check_time.is_some());
        let long_ago = Instant::now().sub(Duration::from_secs(10)).unwrap();
        let first_check = first_check_time
            .unwrap()
            .duration_since(long_ago)
            .unwrap()
            .as_nanos();
        assert_eq!(9, acquired_check_times.len());

        let mut div_by_three = 0;
        let mut div_by_five = 0;
        let mut div_by_seven = 0;
        for time in acquired_check_times {
            // Since next time should be a clean add in the checker we should be able to use nano-time without a problem
            let nanos_of_check = time.duration_since(long_ago).unwrap().as_nanos();
            unix_eprintln!("{nanos_of_check} - {first_check}");
            let diff = nanos_of_check - first_check;
            assert_eq!(0, diff % 1_000_000); // All should be millis expanded to nano time, no clock drift
            let compacted = diff / 1_000_000;
            println!("{diff} {compacted}");

            if compacted % 3 == 0 {
                div_by_three += 1;
            } else if compacted % 5 == 0 {
                div_by_five += 1;
            } else if compacted % 7 == 0 {
                div_by_seven += 1;
            }
        }
        // At 0, 3, 6, 9, 12
        assert_eq!(5, div_by_three);
        // At 5, 10
        assert_eq!(2, div_by_five);
        // At 7, 14 (14 is the projected last 'next_check')
        assert_eq!(2, div_by_seven);
    }

    #[test]
    #[cfg(unix)]
    fn can_real_run_checks() {
        let mut checks = heapless::Vec::new();

        let interval = Duration::from_millis(10_000);
        let _ = checks.push(Check {
            interval: interval.as_millis() as u64,
            check_type: CheckType::Cpu(CpuFormat {
                icon: heapless::String::default(),
                decimals: 2,
            }),
        });
        let now = Instant::now();
        let mut checker = Checker::new(&mut checks);
        let result = checker.run_next(false);
        assert!(result.content.is_some());
        assert!(result.next_check >= now.add(interval).unwrap());
        // If this test takes more than 10 seconds there are other issues
        assert!(result.next_check < now.add(2 * interval).unwrap());
    }
}
