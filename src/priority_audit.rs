//! QNX Neutrino 8.0 Priority Inversion & Worst-Case Execution Time (WCET) Nanosecond Profiler
//!
//! Detects priority inheritance violations:
//! When a real-time thread with high priority (prio >= 63) invokes MsgSend() to a server channel,
//! but the receiving server thread executes at lower priority without boosting (e.g., due to
//! `_NTO_CHF_FIXED_PRIORITY` or non-PI mutexes), medium-priority workloads preempt the server.
//! This results in unbounded priority inversion violating ISO 26262 ASIL-D timing budgets.

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::kev_parser::{COMM_REC_MSG, COMM_REPLY_MSG, COMM_SND_MSG, KevEvent, KevPayload, QNX_TRACE_CLASS_COMM};

pub const REALTIME_PRIORITY_THRESHOLD: u8 = 63; // QNX Neutrino RT boundary

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityInversionViolation {
    pub violation_id: usize,
    pub timestamp_ns: u64,
    pub client_pid: u32,
    pub client_tid: u32,
    pub client_priority: u8,
    pub server_pid: u32,
    pub server_tid: u32,
    pub server_priority: u8,
    pub channel_id: u32,
    pub duration_ns: u64,
    pub severity: String,
    pub root_cause: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelLatencyStats {
    pub channel_id: u32,
    pub sample_count: u64,
    pub min_ns: u64,
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
    pub std_dev_ns: f64,
}

pub struct PriorityAuditEngine {
    /// In-flight requests: (client_pid, client_tid) -> (start_ts, priority, chid, coid, target_pid)
    in_flight_sends: HashMap<(u32, u32), (u64, u8, u32, u32, u32)>,
    /// Histograms per channel_id
    channel_histograms: HashMap<u32, Histogram<u64>>,
    /// Recorded violations
    violations: Vec<PriorityInversionViolation>,
    violation_counter: usize,
}

impl PriorityAuditEngine {
    pub fn new() -> Self {
        Self {
            in_flight_sends: HashMap::new(),
            channel_histograms: HashMap::new(),
            violations: Vec::new(),
            violation_counter: 1,
        }
    }

    pub fn feed_event(&mut self, ev: &KevEvent) {
        if ev.class != QNX_TRACE_CLASS_COMM {
            return;
        }

        match &ev.payload {
            KevPayload::CommMsg {
                pid,
                tid,
                chid,
                coid,
                target_pid,
                target_tid: _,
                priority,
                ..
            } => {
                let client_key = (*pid, *tid);

                if ev.event_code == COMM_SND_MSG {
                    self.in_flight_sends.insert(
                        client_key,
                        (ev.timestamp_ns, *priority, *chid, *coid, *target_pid),
                    );
                } else if ev.event_code == COMM_REC_MSG {
                    // Receiver thread begins handling message.
                    // Check if client is high-prio while receiver runs at lower priority
                    for (client, (start_ts, c_prio, c_chid, _, _)) in &self.in_flight_sends {
                        if *c_chid == *chid && *c_prio >= REALTIME_PRIORITY_THRESHOLD {
                            if *priority < *c_prio {
                                let duration = ev.timestamp_ns.saturating_sub(*start_ts);
                                let root_cause = if *priority < 20 {
                                    format!(
                                        "Channel {} missing Priority Inheritance! High-prio RT client (prio {}) blocked while server runs at background prio {}. Likely caused by _NTO_CHF_FIXED_PRIORITY flag or non-PI dispatch loop.",
                                        chid, c_prio, priority
                                    )
                                } else {
                                    format!(
                                        "Unbounded Priority Inversion: Server runs at priority {} without inheriting client priority {}.",
                                        priority, c_prio
                                    )
                                };

                                self.violations.push(PriorityInversionViolation {
                                    violation_id: self.violation_counter,
                                    timestamp_ns: ev.timestamp_ns,
                                    client_pid: client.0,
                                    client_tid: client.1,
                                    client_priority: *c_prio,
                                    server_pid: *pid,
                                    server_tid: *tid,
                                    server_priority: *priority,
                                    channel_id: *chid,
                                    duration_ns: duration,
                                    severity: if *c_prio >= 100 { "CRITICAL".to_string() } else { "HIGH".to_string() },
                                    root_cause,
                                });
                                self.violation_counter += 1;
                            }
                        }
                    }
                } else if ev.event_code == COMM_REPLY_MSG {
                    if let Some((start_ts, _c_prio, chid, _coid, _)) = self.in_flight_sends.remove(&client_key) {
                        let latency_ns = ev.timestamp_ns.saturating_sub(start_ts);
                        let hist = self
                            .channel_histograms
                            .entry(chid)
                            .or_insert_with(|| Histogram::<u64>::new_with_bounds(1, 100_000_000_000, 3).unwrap());
                        let _ = hist.record(latency_ns.max(1));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn get_violations(&self) -> &[PriorityInversionViolation] {
        &self.violations
    }

    pub fn get_latency_stats(&self) -> Vec<ChannelLatencyStats> {
        let mut stats = Vec::new();
        for (&chid, hist) in &self.channel_histograms {
            stats.push(ChannelLatencyStats {
                channel_id: chid,
                sample_count: hist.len(),
                min_ns: hist.min(),
                p50_ns: hist.value_at_quantile(0.50),
                p90_ns: hist.value_at_quantile(0.90),
                p99_ns: hist.value_at_quantile(0.99),
                p999_ns: hist.value_at_quantile(0.999),
                max_ns: hist.max(),
                std_dev_ns: hist.stdev(),
            });
        }
        stats.sort_by_key(|s| s.channel_id);
        stats
    }
}
