use crate::error::Error;
use crate::status::sys::{find_byte, find_in_haystack};

pub const MEM_LOAD_FILE: &str = "/proc/meminfo\0";

#[derive(Debug, Copy, Clone, Default)]
pub struct Data {
    pub total: u64,
    pub available: u64,
    pub swapped: u64,
}

#[allow(unsafe_code)]
#[inline]
pub fn read_mem_info() -> Result<Data, Error> {
    let buf = tiny_std::fs::read(MEM_LOAD_FILE)?;
    parse_raw(&buf)
}

pub fn parse_raw(mem_info: &[u8]) -> Result<Data, Error> {
    let total = parse_line_containing(mem_info, b"MemTotal")?;
    let avail = parse_line_containing(mem_info, b"MemAvailable")?;
    let swap_total = parse_line_containing(mem_info, b"SwapTotal")?;
    let swap_free = parse_line_containing(mem_info, b"SwapFree")?;
    Ok(Data {
        total,
        available: avail,
        swapped: swap_total - swap_free,
    })
}

fn parse_line_containing(haystack: &[u8], needle: &[u8]) -> Result<u64, Error> {
    if let Some(start) = find_in_haystack(haystack, needle) {
        // We're not even close to the end here
        let sub_target = &haystack[start..];
        parse_kb_value(sub_target)
    } else {
        Err(Error::MemParseError(
            "Could not find search param in mem search",
        ))
    }
}

fn parse_kb_value(target: &[u8]) -> Result<u64, Error> {
    if let Some(next_space_ind) = find_byte(b' ', target) {
        let mut next_non_space = next_space_ind;
        while target[next_non_space] == b' ' {
            next_non_space += 1;
        }
        if let Some(next_space) = find_byte(b' ', &target[next_non_space..]) {
            return atoi::atoi(&target[next_non_space..next_non_space + next_space])
                .ok_or(Error::MemParseError("Failed to parse kb value of mem info"));
        }
    }
    Err(Error::MemParseError("Failed to parse kb value of mem info"))
}

#[cfg(test)]
mod tests {
    use super::parse_raw;

    #[test]
    pub fn parse_mem_raw() {
        let input = b"MemTotal:       16314980 kB
MemFree:          445304 kB
MemAvailable:   10723148 kB
Buffers:          281968 kB
Cached:          9769372 kB
SwapCached:           28 kB
Active:          3568124 kB
Inactive:       11113988 kB
Active(anon):      32848 kB
Inactive(anon):  4726532 kB
Active(file):    3535276 kB
Inactive(file):  6387456 kB
Unevictable:           0 kB
Mlocked:               0 kB
SwapTotal:      33538044 kB
SwapFree:       33536252 kB
Dirty:             26704 kB
Writeback:             0 kB
AnonPages:       4630800 kB
Mapped:          1768992 kB
Shmem:            128608 kB
KReclaimable:     692040 kB
Slab:             839536 kB
SReclaimable:     692040 kB
SUnreclaim:       147496 kB
KernelStack:       12816 kB
PageTables:        45380 kB
NFS_Unstable:          0 kB
Bounce:                0 kB
WritebackTmp:          0 kB
CommitLimit:    41695532 kB
Committed_AS:   11331124 kB
VmallocTotal:   34359738367 kB
VmallocUsed:       77676 kB
VmallocChunk:          0 kB
Percpu:             3776 kB
HardwareCorrupted:     0 kB
AnonHugePages:         0 kB
ShmemHugePages:        0 kB
ShmemPmdMapped:        0 kB
FileHugePages:         0 kB
FilePmdMapped:         0 kB
CmaTotal:              0 kB
CmaFree:               0 kB
HugePages_Total:       0
HugePages_Free:        0
HugePages_Rsvd:        0
HugePages_Surp:        0
Hugepagesize:       2048 kB
Hugetlb:               0 kB
DirectMap4k:      652008 kB
DirectMap2M:    13957120 kB
DirectMap1G:     2097152 kB";
        let info = parse_raw(input).unwrap();
        assert_eq!(info.total, 16_314_980);
        assert_eq!(info.available, 10_723_148);
        assert_eq!(info.swapped, 1792);
    }
}
