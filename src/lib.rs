//! QNX Neutrino 8.0 Kernel Event Trace (.kev) Diagnostic Engine
//! High-performance Rust analyzer for real-time IPC, deadlocks, priority inversions, and APS partitions.

pub mod aps_budget;
pub mod html_report;
pub mod kev_parser;
pub mod priority_audit;
pub mod sarif;
pub mod srr_graph;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use aps_budget::{ApsAnomaly, ApsAuditEngine, PartitionHealth};
use kev_parser::{
    COMM_REC_MSG, COMM_REPLY_MSG, COMM_SND_MSG, KevEvent, KevHeader, KevPayload, KevTrace,
    QNX_TRACE_CLASS_APS, QNX_TRACE_CLASS_COMM,
};
use priority_audit::{ChannelLatencyStats, PriorityAuditEngine, PriorityInversionViolation};
use srr_graph::{DeadlockCycle, SrrWaitGraph};

pub struct AuditReport {
    pub total_events_analyzed: usize,
    pub deadlocks: Vec<DeadlockCycle>,
    pub priority_inversions: Vec<PriorityInversionViolation>,
    pub latency_stats: Vec<ChannelLatencyStats>,
    pub aps_partitions: Vec<PartitionHealth>,
    pub aps_anomalies: Vec<ApsAnomaly>,
}

/// Run end-to-end audit on a parsed QNX KEV trace
pub fn audit_trace(trace: &KevTrace) -> AuditReport {
    let mut srr_graph = SrrWaitGraph::new();
    let mut prio_engine = PriorityAuditEngine::new();
    let mut aps_engine = ApsAuditEngine::new();

    let mut detected_deadlocks = Vec::new();

    for ev in &trace.events {
        srr_graph.feed_event(ev);
        prio_engine.feed_event(ev);
        aps_engine.feed_event(ev);

        // Periodically run Tarjan SCC cycle check when a new send event arrives
        if ev.class == QNX_TRACE_CLASS_COMM && ev.event_code == COMM_SND_MSG {
            let cycles = srr_graph.detect_deadlocks(ev.timestamp_ns);
            for c in cycles {
                let already_exists = detected_deadlocks.iter().any(|existing: &DeadlockCycle| {
                    let mut s1 = existing.participants.clone();
                    let mut s2 = c.participants.clone();
                    s1.sort_by_key(|p| (p.pid, p.tid));
                    s2.sort_by_key(|p| (p.pid, p.tid));
                    s1 == s2 && existing.channels_involved == c.channels_involved
                });
                if !already_exists {
                    detected_deadlocks.push(c);
                }
            }
        }
    }

    // Final deadlock sweep
    let final_ts = trace.events.last().map(|e| e.timestamp_ns).unwrap_or(0);
    let final_cycles = srr_graph.detect_deadlocks(final_ts);
    for c in final_cycles {
        let already_exists = detected_deadlocks.iter().any(|existing: &DeadlockCycle| {
            let mut s1 = existing.participants.clone();
            let mut s2 = c.participants.clone();
            s1.sort_by_key(|p| (p.pid, p.tid));
            s2.sort_by_key(|p| (p.pid, p.tid));
            s1 == s2 && existing.channels_involved == c.channels_involved
        });
        if !already_exists {
            detected_deadlocks.push(c);
        }
    }

    AuditReport {
        total_events_analyzed: trace.events.len(),
        deadlocks: detected_deadlocks,
        priority_inversions: prio_engine.get_violations().to_vec(),
        latency_stats: prio_engine.get_latency_stats(),
        aps_partitions: aps_engine.summarize(),
        aps_anomalies: aps_engine.get_anomalies().to_vec(),
    }
}

/// Load a .kev binary trace from disk and audit
pub fn audit_kev_file(path: &Path) -> std::io::Result<AuditReport> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let trace = KevTrace::read_from(&mut reader)?;
    Ok(audit_trace(&trace))
}

/// Synthesize a rich authentic QNX 8.0 .kev trace containing:
/// 1. A 4-process circular IPC deadlock (ADAS -> Brake -> Sensor -> Diag -> ADAS)
/// 2. Unbounded priority inversion (_NTO_CHF_FIXED_PRIORITY missing PI)
/// 3. Normal high-throughput IPC traffic for WCET nanosecond profiling
/// 4. Adaptive Partitioning budget overruns
pub fn generate_synthetic_qnx8_trace() -> KevTrace {
    let mut header = KevHeader::default();
    header.num_cpus = 8;
    header.timestamp_freq_hz = 1_000_000_000; // 1 GHz
    let mut trace = KevTrace::new(header);

    let mut current_ts = 1_000_000_000u64; // start at 1.0s

    // 1. Setup normal IPC traffic across channels 10, 20, 30 (1000 messages)
    for i in 0..600 {
        current_ts += 15_000 + (i % 7) * 2_500; // ~15-30 us
        let chid = 10 + (i % 3) * 10;
        let prio = 20 + (i % 10) as u8;

        // Send Msg
        trace.add_event(KevEvent {
            timestamp_ns: current_ts,
            cpu_id: (i % 4) as u8,
            class: QNX_TRACE_CLASS_COMM,
            event_code: COMM_SND_MSG,
            payload: KevPayload::CommMsg {
                pid: 100 + (i % 5) as u32,
                tid: 1,
                chid: chid as u32,
                coid: 1,
                target_pid: 50 + (i % 3) as u32,
                target_tid: 1,
                msg_size: 128,
                priority: prio,
            },
        });

        // Receive Msg
        current_ts += 5_000 + (i % 4) * 1_200;
        trace.add_event(KevEvent {
            timestamp_ns: current_ts,
            cpu_id: (i % 4) as u8,
            class: QNX_TRACE_CLASS_COMM,
            event_code: COMM_REC_MSG,
            payload: KevPayload::CommMsg {
                pid: 50 + (i % 3) as u32,
                tid: 1,
                chid: chid as u32,
                coid: 1,
                target_pid: 100 + (i % 5) as u32,
                target_tid: 1,
                msg_size: 128,
                priority: prio,
            },
        });

        // Reply Msg
        current_ts += 8_000 + (i % 5) * 800;
        trace.add_event(KevEvent {
            timestamp_ns: current_ts,
            cpu_id: (i % 4) as u8,
            class: QNX_TRACE_CLASS_COMM,
            event_code: COMM_REPLY_MSG,
            payload: KevPayload::CommMsg {
                pid: 100 + (i % 5) as u32,
                tid: 1,
                chid: chid as u32,
                coid: 1,
                target_pid: 50 + (i % 3) as u32,
                target_tid: 1,
                msg_size: 64,
                priority: prio,
            },
        });
    }

    // 2. Inject Priority Inversion: RT client (Prio 63) calls server with Fixed Prio 10
    current_ts += 100_000;
    trace.add_event(KevEvent {
        timestamp_ns: current_ts,
        cpu_id: 1,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 1001, // ADAS RT Controller
            tid: 1,
            chid: 88,  // Actuator Resource Manager
            coid: 1,
            target_pid: 2001,
            target_tid: 1,
            msg_size: 256,
            priority: 63, // Real-time priority!
        },
    });

    current_ts += 25_000;
    trace.add_event(KevEvent {
        timestamp_ns: current_ts,
        cpu_id: 2,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_REC_MSG,
        payload: KevPayload::CommMsg {
            pid: 2001, // Actuator ResMgr
            tid: 1,
            chid: 88,
            coid: 1,
            target_pid: 1001,
            target_tid: 1,
            msg_size: 256,
            priority: 10, // Running at background prio without PI!
        },
    });

    // 3. Inject Multi-Process Circular IPC Deadlock:
    // Process 1001 -> Channel 88 (Server 2001)
    // Server 2001 -> Channel 89 (Server 2002)
    // Server 2002 -> Channel 90 (Server 3001)
    // Server 3001 -> Channel 91 (Process 1001) -> CYCLE!
    current_ts += 200_000;
    trace.add_event(KevEvent {
        timestamp_ns: current_ts,
        cpu_id: 1,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 2001,
            tid: 1,
            chid: 89,
            coid: 2,
            target_pid: 2002,
            target_tid: 1,
            msg_size: 64,
            priority: 10,
        },
    });

    current_ts += 10_000;
    trace.add_event(KevEvent {
        timestamp_ns: current_ts,
        cpu_id: 2,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 2002,
            tid: 1,
            chid: 90,
            coid: 3,
            target_pid: 3001,
            target_tid: 1,
            msg_size: 64,
            priority: 10,
        },
    });

    current_ts += 10_000;
    trace.add_event(KevEvent {
        timestamp_ns: current_ts,
        cpu_id: 3,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 3001,
            tid: 1,
            chid: 91,
            coid: 4,
            target_pid: 1001, // Cycle back to 1001!
            target_tid: 1,
            msg_size: 64,
            priority: 10,
        },
    });

    // 4. Inject Adaptive Partitioning events
    current_ts += 500_000;
    trace.add_event(KevEvent {
        timestamp_ns: current_ts,
        cpu_id: 0,
        class: QNX_TRACE_CLASS_APS,
        event_code: 1,
        payload: KevPayload::ApsBudget {
            partition_id: 1, // Safety_ADAS_Critical
            budget_percent: 50,
            used_percent: 48,
            critical_budget_ms: 100,
            critical_used_ms: 22,
            num_overruns: 0,
        },
    });

    trace.add_event(KevEvent {
        timestamp_ns: current_ts + 1_000_000,
        cpu_id: 1,
        class: QNX_TRACE_CLASS_APS,
        event_code: 2, // Overrun
        payload: KevPayload::ApsBudget {
            partition_id: 3, // Infotainment
            budget_percent: 20,
            used_percent: 38,
            critical_budget_ms: 50,
            critical_used_ms: 72, // Exceeded!
            num_overruns: 3,
        },
    });

    trace
}
