use tiny_bench::*;

use pgwm_core::config::NUM_TILING_MODIFIERS;
use pgwm_core::geometry::layout::Layout;

const VERTICAL_TILING_MODIFIERS: [f32; NUM_TILING_MODIFIERS] = [1.0; NUM_TILING_MODIFIERS];

pub fn main() {
    let dims = VERTICAL_TILING_MODIFIERS;
    let left = 2.0;
    let center = 2.0;
    bench_labeled("Layout left single window", || {
        Layout::LeftLeader.calculate_dimensions(
            black_box(1000),
            black_box(1000),
            black_box(5),
            black_box(3),
            black_box(20),
            black_box(true),
            black_box(1),
            black_box(&dims),
            black_box(left),
            black_box(center),
        )
    });
    let dims = VERTICAL_TILING_MODIFIERS;
    let left = 2.0;
    let center = 2.0;
    bench_labeled("Layout left two windows", || {
        Layout::LeftLeader.calculate_dimensions(
            black_box(1000),
            black_box(1000),
            black_box(5),
            black_box(3),
            black_box(20),
            black_box(true),
            black_box(2),
            black_box(&dims),
            black_box(left),
            black_box(center),
        )
    });
    let dims = VERTICAL_TILING_MODIFIERS;
    let len = dims.len();
    let left = 2.0;
    let center = 2.0;
    bench_labeled("Layout left, full windows", || {
        Layout::LeftLeader.calculate_dimensions(
            black_box(1000),
            black_box(1000),
            black_box(5),
            black_box(3),
            black_box(20),
            black_box(true),
            black_box(len),
            black_box(&dims),
            black_box(left),
            black_box(center),
        )
    });
    let dims = VERTICAL_TILING_MODIFIERS;
    let left = 2.0;
    let center = 2.0;
    bench_labeled("Layout center single window", || {
        Layout::CenterLeader.calculate_dimensions(
            black_box(1000),
            black_box(1000),
            black_box(5),
            black_box(3),
            black_box(20),
            black_box(true),
            black_box(1),
            black_box(&dims),
            black_box(left),
            black_box(center),
        )
    });
    let dims = VERTICAL_TILING_MODIFIERS;
    let left = 2.0;
    let center = 2.0;
    bench_labeled("Layout center two windows", || {
        Layout::CenterLeader.calculate_dimensions(
            black_box(1000),
            black_box(1000),
            black_box(5),
            black_box(3),
            black_box(20),
            black_box(true),
            black_box(2),
            black_box(&dims),
            black_box(left),
            black_box(center),
        )
    });
    let dims = VERTICAL_TILING_MODIFIERS;
    let len = dims.len();
    let left = 2.0;
    let center = 2.0;
    bench_labeled("Layout center, full windows", || {
        Layout::CenterLeader.calculate_dimensions(
            black_box(1000),
            black_box(1000),
            black_box(5),
            black_box(3),
            black_box(20),
            black_box(true),
            black_box(len),
            black_box(&dims),
            black_box(left),
            black_box(center),
        )
    });
    let input = b"cpu  81196 0 15968 1477813 10828 1617 874 0 0 0
cpu0 13050 0 2577 247458 1336 210 66 0 0 0
cpu1 14339 0 2606 244422 2949 216 176 0 0 0
cpu2 13980 0 2718 245844 1771 265 57 0 0 0
cpu3 13270 0 3024 245911 1715 463 446 0 0 0
cpu4 12882 0 2580 247376 1449 279 83 0 0 0
cpu5 13673 0 2460 246800 1606 182 44 0 0 0
intr 3669331 9 0 0 0 0 0 0 0 1 0 0 0 0 0 0 0 5 991 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 75319 16 4777 5303 4972 6099 4929 5670 10663 42 461 1502524 32245 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
ctxt 7432226
btime 1641118164
processes 10723
procs_running 2
procs_blocked 0
softirq 2476480 10333 106783 612 1503016 73220 0 10340 369864 832 401480\n".to_owned();
    bench_labeled("parse cpu stats", || {
        pgwm_core::status::sys::cpu::parse_raw(black_box(&input))
    });
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
    bench_labeled("parse_mem_raw", || {
        pgwm_core::status::sys::mem::parse_raw(black_box(input))
    });
    let input = "
TcpExt: SyncookiesSent SyncookiesRecv SyncookiesFailed EmbryonicRsts PruneCalled RcvPruned OfoPruned OutOfWindowIcmps LockDroppedIcmps ArpFilter TW TWRecycled TWKilled PAWSActive PAWSEstab DelayedACKs DelayedACKLocked DelayedACKLost ListenOverflows ListenDrops TCPHPHits TCPPureAcks TCPHPAcks TCPRenoRecovery TCPSackRecovery TCPSACKReneging TCPSACKReorder TCPRenoReorder TCPTSReorder TCPFullUndo TCPPartialUndo TCPDSACKUndo TCPLossUndo TCPLostRetransmit TCPRenoFailures TCPSackFailures TCPLossFailures TCPFastRetrans TCPSlowStartRetrans TCPTimeouts TCPLossProbes TCPLossProbeRecovery TCPRenoRecoveryFail TCPSackRecoveryFail TCPRcvCollapsed TCPBacklogCoalesce TCPDSACKOldSent TCPDSACKOfoSent TCPDSACKRecv TCPDSACKOfoRecv TCPAbortOnData TCPAbortOnClose TCPAbortOnMemory TCPAbortOnTimeout TCPAbortOnLinger TCPAbortFailed TCPMemoryPressures TCPMemoryPressuresChrono TCPSACKDiscard TCPDSACKIgnoredOld TCPDSACKIgnoredNoUndo TCPSpuriousRTOs TCPMD5NotFound TCPMD5Unexpected TCPMD5Failure TCPSackShifted TCPSackMerged TCPSackShiftFallback TCPBacklogDrop PFMemallocDrop TCPMinTTLDrop TCPDeferAcceptDrop IPReversePathFilter TCPTimeWaitOverflow TCPReqQFullDoCookies TCPReqQFullDrop TCPRetransFail TCPRcvCoalesce TCPOFOQueue TCPOFODrop TCPOFOMerge TCPChallengeACK TCPSYNChallenge TCPFastOpenActive TCPFastOpenActiveFail TCPFastOpenPassive TCPFastOpenPassiveFail TCPFastOpenListenOverflow TCPFastOpenCookieReqd TCPFastOpenBlackhole TCPSpuriousRtxHostQueues BusyPollRxPackets TCPAutoCorking TCPFromZeroWindowAdv TCPToZeroWindowAdv TCPWantZeroWindowAdv TCPSynRetrans TCPOrigDataSent TCPHystartTrainDetect TCPHystartTrainCwnd TCPHystartDelayDetect TCPHystartDelayCwnd TCPACKSkippedSynRecv TCPACKSkippedPAWS TCPACKSkippedSeq TCPACKSkippedFinWait2 TCPACKSkippedTimeWait TCPACKSkippedChallenge TCPWinProbe TCPKeepAlive TCPMTUPFail TCPMTUPSuccess TCPDelivered TCPDeliveredCE TCPAckCompressed TCPZeroWindowDrop TCPRcvQDrop TCPWqueueTooBig TCPFastOpenPassiveAltKey TcpTimeoutRehash TcpDuplicateDataRehash TCPDSACKRecvSegs TCPDSACKIgnoredDubious TCPMigrateReqSuccess TCPMigrateReqFailure
TcpExt: 0 0 0 0 0 0 0 0 0 0 969 0 0 0 0 992 0 13 0 0 451373 17417 67395 0 1 0 1 0 0 0 0 0 0 0 0 0 0 1 0 0 25 1 0 0 0 1335 13 0 1 0 159 49 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 3 0 0 0 0 0 0 0 0 0 100357 973 0 0 0 0 0 0 0 0 0 0 0 0 0 6111 15 15 100 0 123612 24 541 0 0 0 0 10 0 0 0 0 7304 0 0 125291 0 162 0 0 0 0 0 0 1 0 0 0
IpExt: InNoRoutes InTruncatedPkts InMcastPkts OutMcastPkts InBcastPkts OutBcastPkts InOctets OutOctets InMcastOctets OutMcastOctets InBcastOctets OutBcastOctets InCsumErrors InNoECTPkts InECT1Pkts InECT0Pkts InCEPkts ReasmOverlaps
IpExt: 0 0 17545 5939 34763 2590 562744872 81033289 3234956 824524 9101842 186412 0 550730 0 0 0 0
MPTcpExt: MPCapableSYNRX MPCapableSYNTX MPCapableSYNACKRX MPCapableACKRX MPCapableFallbackACK MPCapableFallbackSYNACK MPFallbackTokenInit MPTCPRetrans MPJoinNoTokenFound MPJoinSynRx MPJoinSynAckRx MPJoinSynAckHMacFailure MPJoinAckRx MPJoinAckHMacFailure DSSNotMatching InfiniteMapRx DSSNoMatchTCP DataCsumErr OFOQueueTail OFOQueue OFOMerge NoDSSInWindow DuplicateData AddAddr EchoAdd PortAdd AddAddrDrop MPJoinPortSynRx MPJoinPortSynAckRx MPJoinPortAckRx MismatchPortSynRx MismatchPortAckRx RmAddr RmAddrDrop RmSubflow MPPrioTx MPPrioRx MPFailTx MPFailRx RcvPruned SubflowStale SubflowRecover
MPTcpExt: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
";
    bench_labeled("parse_net_raw", || {
        pgwm_core::status::sys::net::parse_raw(black_box(input.as_bytes()))
    });
}
