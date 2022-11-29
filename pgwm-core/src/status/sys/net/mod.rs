use crate::error::Error;
use crate::status::sys::{find_byte, find_in_haystack};

const NET_STAT: &str = "/proc/net/netstat\0";

#[allow(unsafe_code)]
pub fn read_net_stats() -> Result<Data, Error> {
    let buf = tiny_std::fs::read(NET_STAT)?;
    parse_raw(&buf)
}

pub fn parse_raw(raw_data: &[u8]) -> Result<Data, Error> {
    if let Some(label_line_start) = find_in_haystack(raw_data, b"IpExt: ") {
        if let Some(real_line_start) =
            find_in_haystack(&raw_data[label_line_start + 1..], b"IpExt: ")
        {
            let real_line_start = label_line_start + real_line_start;
            let mut prev_ind = real_line_start;
            let mut it = 0;
            let mut bytes_in = 0;
            while let Some(space_ind) = find_byte(b' ', &raw_data[prev_ind..]) {
                prev_ind += if space_ind == 0 {
                    1
                } else {
                    let target = &raw_data[prev_ind..prev_ind + space_ind];
                    if it == 7 {
                        bytes_in = atoi::atoi::<u64>(target).ok_or(Error::NetStatParseError)?;
                    } else if it == 8 {
                        return Ok(Data {
                            bytes_in,
                            bytes_out: atoi::atoi::<u64>(target).ok_or(Error::NetStatParseError)?,
                        });
                    }
                    it += 1;
                    space_ind
                };
            }
        }
    }
    Err(Error::NetStatParseError)
}

#[derive(Clone, Copy, Debug)]
pub struct Data {
    pub bytes_in: u64,
    pub bytes_out: u64,
}

#[cfg(test)]
mod tests {
    use super::parse_raw;

    #[test]
    fn test_read_raw() {
        let input = b"TcpExt: SyncookiesSent SyncookiesRecv SyncookiesFailed EmbryonicRsts PruneCalled RcvPruned OfoPruned OutOfWindowIcmps LockDroppedIcmps ArpFilter TW TWRecycled TWKilled PAWSActive PAWSEstab DelayedACKs DelayedACKLocked DelayedACKLost ListenOverflows ListenDrops TCPHPHits TCPPureAcks TCPHPAcks TCPRenoRecovery TCPSackRecovery TCPSACKReneging TCPSACKReorder TCPRenoReorder TCPTSReorder TCPFullUndo TCPPartialUndo TCPDSACKUndo TCPLossUndo TCPLostRetransmit TCPRenoFailures TCPSackFailures TCPLossFailures TCPFastRetrans TCPSlowStartRetrans TCPTimeouts TCPLossProbes TCPLossProbeRecovery TCPRenoRecoveryFail TCPSackRecoveryFail TCPRcvCollapsed TCPBacklogCoalesce TCPDSACKOldSent TCPDSACKOfoSent TCPDSACKRecv TCPDSACKOfoRecv TCPAbortOnData TCPAbortOnClose TCPAbortOnMemory TCPAbortOnTimeout TCPAbortOnLinger TCPAbortFailed TCPMemoryPressures TCPMemoryPressuresChrono TCPSACKDiscard TCPDSACKIgnoredOld TCPDSACKIgnoredNoUndo TCPSpuriousRTOs TCPMD5NotFound TCPMD5Unexpected TCPMD5Failure TCPSackShifted TCPSackMerged TCPSackShiftFallback TCPBacklogDrop PFMemallocDrop TCPMinTTLDrop TCPDeferAcceptDrop IPReversePathFilter TCPTimeWaitOverflow TCPReqQFullDoCookies TCPReqQFullDrop TCPRetransFail TCPRcvCoalesce TCPOFOQueue TCPOFODrop TCPOFOMerge TCPChallengeACK TCPSYNChallenge TCPFastOpenActive TCPFastOpenActiveFail TCPFastOpenPassive TCPFastOpenPassiveFail TCPFastOpenListenOverflow TCPFastOpenCookieReqd TCPFastOpenBlackhole TCPSpuriousRtxHostQueues BusyPollRxPackets TCPAutoCorking TCPFromZeroWindowAdv TCPToZeroWindowAdv TCPWantZeroWindowAdv TCPSynRetrans TCPOrigDataSent TCPHystartTrainDetect TCPHystartTrainCwnd TCPHystartDelayDetect TCPHystartDelayCwnd TCPACKSkippedSynRecv TCPACKSkippedPAWS TCPACKSkippedSeq TCPACKSkippedFinWait2 TCPACKSkippedTimeWait TCPACKSkippedChallenge TCPWinProbe TCPKeepAlive TCPMTUPFail TCPMTUPSuccess TCPDelivered TCPDeliveredCE TCPAckCompressed TCPZeroWindowDrop TCPRcvQDrop TCPWqueueTooBig TCPFastOpenPassiveAltKey TcpTimeoutRehash TcpDuplicateDataRehash TCPDSACKRecvSegs TCPDSACKIgnoredDubious TCPMigrateReqSuccess TCPMigrateReqFailure
TcpExt: 0 0 0 0 0 0 0 0 0 0 388 0 0 0 0 557 1 22 0 0 65099 9108 22858 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 5 0 0 0 0 176 22 0 0 0 115 19 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 25951 2 0 0 0 0 0 0 0 0 0 0 0 0 0 1290 0 0 2 0 32384 18 435 0 0 0 0 0 0 0 0 0 4280 0 0 33195 0 0 0 0 0 0 0 0 0 0 0 0
IpExt: InNoRoutes InTruncatedPkts InMcastPkts OutMcastPkts InBcastPkts OutBcastPkts InOctets OutOctets InMcastOctets OutMcastOctets InBcastOctets OutBcastOctets InCsumErrors InNoECTPkts InECT1Pkts InECT0Pkts InCEPkts ReasmOverlaps
IpExt: 0 0 9761 3313 21175 1332 145351181 7537882 1855121 496314 5656604 95840 0 242636 0 0 0 0
MPTcpExt: MPCapableSYNRX MPCapableSYNTX MPCapableSYNACKRX MPCapableACKRX MPCapableFallbackACK MPCapableFallbackSYNACK MPFallbackTokenInit MPTCPRetrans MPJoinNoTokenFound MPJoinSynRx MPJoinSynAckRx MPJoinSynAckHMacFailure MPJoinAckRx MPJoinAckHMacFailure DSSNotMatching InfiniteMapRx DSSNoMatchTCP DataCsumErr OFOQueueTail OFOQueue OFOMerge NoDSSInWindow DuplicateData AddAddr EchoAdd PortAdd AddAddrDrop MPJoinPortSynRx MPJoinPortSynAckRx MPJoinPortAckRx MismatchPortSynRx MismatchPortAckRx RmAddr RmAddrDrop RmSubflow MPPrioTx MPPrioRx MPFailTx MPFailRx RcvPruned SubflowStale SubflowRecover
MPTcpExt: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let net_data = parse_raw(input).unwrap();
        assert_eq!(145_351_181, net_data.bytes_in);
        assert_eq!(7_537_882, net_data.bytes_out);
    }
}
