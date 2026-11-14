//! Comprehensive Integration Test Suite for QNX KEV Doctor

use std::io::Cursor;
use tempfile::NamedTempFile;

use qnx_kev_doctor::aps_budget::ApsAuditEngine;
use qnx_kev_doctor::html_report::generate_html_report;
use qnx_kev_doctor::kev_parser::{
    COMM_REC_MSG, COMM_SND_MSG, KevEvent, KevPayload, KevTrace,
    QNX_TRACE_CLASS_APS, QNX_TRACE_CLASS_COMM,
};
use qnx_kev_doctor::priority_audit::PriorityAuditEngine;
use qnx_kev_doctor::sarif::generate_sarif_report;
use qnx_kev_doctor::srr_graph::SrrWaitGraph;
use qnx_kev_doctor::{audit_trace, generate_synthetic_qnx8_trace};

#[test]
fn test_kev_binary_roundtrip() {
    let trace = generate_synthetic_qnx8_trace();
    let mut buffer = Vec::new();

    trace.write_to(&mut buffer).expect("Serialization failed");
    assert!(!buffer.is_empty());

    let mut cursor = Cursor::new(buffer);
    let decoded = KevTrace::read_from(&mut cursor).expect("Deserialization failed");

    assert_eq!(decoded.header.magic, trace.header.magic);
    assert_eq!(decoded.events.len(), trace.events.len());
    assert_eq!(decoded.events[0].timestamp_ns, trace.events[0].timestamp_ns);
}

#[test]
fn test_tarjan_scc_deadlock_detection() {
    let mut graph = SrrWaitGraph::new();

    // Setup 3-way circular dependency:
    // P1 (TID 1) -> P2 (TID 1) on Chid 100
    // P2 (TID 1) -> P3 (TID 1) on Chid 101
    // P3 (TID 1) -> P1 (TID 1) on Chid 102
    let ev1 = KevEvent {
        timestamp_ns: 1_000_000,
        cpu_id: 0,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 1,
            tid: 1,
            chid: 100,
            coid: 1,
            target_pid: 2,
            target_tid: 1,
            msg_size: 32,
            priority: 10,
        },
    };
    let ev2 = KevEvent {
        timestamp_ns: 1_005_000,
        cpu_id: 1,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 2,
            tid: 1,
            chid: 101,
            coid: 2,
            target_pid: 3,
            target_tid: 1,
            msg_size: 32,
            priority: 10,
        },
    };
    let ev3 = KevEvent {
        timestamp_ns: 1_010_000,
        cpu_id: 2,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 3,
            tid: 1,
            chid: 102,
            coid: 3,
            target_pid: 1,
            target_tid: 1,
            msg_size: 32,
            priority: 10,
        },
    };

    graph.feed_event(&ev1);
    assert_eq!(graph.detect_deadlocks(1_000_000).len(), 0);

    graph.feed_event(&ev2);
    assert_eq!(graph.detect_deadlocks(1_005_000).len(), 0);

    graph.feed_event(&ev3);
    let deadlocks = graph.detect_deadlocks(1_010_000);
    assert_eq!(deadlocks.len(), 1, "Tarjan SCC must detect the 3-process cycle");
    assert_eq!(deadlocks[0].participants.len(), 3);
}

#[test]
fn test_priority_inversion_detection() {
    let mut engine = PriorityAuditEngine::new();

    // High prio RT client sends to server
    let ev_send = KevEvent {
        timestamp_ns: 2_000_000,
        cpu_id: 0,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_SND_MSG,
        payload: KevPayload::CommMsg {
            pid: 999,
            tid: 1,
            chid: 50,
            coid: 1,
            target_pid: 100,
            target_tid: 1,
            msg_size: 64,
            priority: 120, // High real-time priority
        },
    };

    // Server receives but runs at low priority (10)
    let ev_rec = KevEvent {
        timestamp_ns: 2_050_000,
        cpu_id: 1,
        class: QNX_TRACE_CLASS_COMM,
        event_code: COMM_REC_MSG,
        payload: KevPayload::CommMsg {
            pid: 100,
            tid: 1,
            chid: 50,
            coid: 1,
            target_pid: 999,
            target_tid: 1,
            msg_size: 64,
            priority: 10, // Inversion!
        },
    };

    engine.feed_event(&ev_send);
    engine.feed_event(&ev_rec);

    let violations = engine.get_violations();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].client_priority, 120);
    assert_eq!(violations[0].server_priority, 10);
    assert_eq!(violations[0].severity, "CRITICAL");
}

#[test]
fn test_aps_anomaly_detection() {
    let mut engine = ApsAuditEngine::new();

    let ev = KevEvent {
        timestamp_ns: 3_000_000,
        cpu_id: 0,
        class: QNX_TRACE_CLASS_APS,
        event_code: 2,
        payload: KevPayload::ApsBudget {
            partition_id: 2,
            budget_percent: 25,
            used_percent: 45,
            critical_budget_ms: 50,
            critical_used_ms: 80, // Overrun
            num_overruns: 2,
        },
    };

    engine.feed_event(&ev);
    let anomalies = engine.get_anomalies();
    assert!(!anomalies.is_empty());
}

#[test]
fn test_full_pipeline_and_exporters() {
    let trace = generate_synthetic_qnx8_trace();
    let report = audit_trace(&trace);

    assert!(report.total_events_analyzed > 500);
    assert!(!report.deadlocks.is_empty(), "Synthetic trace must contain deadlock");
    assert!(!report.priority_inversions.is_empty(), "Synthetic trace must contain priority inversion");
    assert!(!report.latency_stats.is_empty(), "WCET histograms must be populated");

    // SARIF export test
    let sarif_file = NamedTempFile::new().unwrap();
    generate_sarif_report(
        &report.deadlocks,
        &report.priority_inversions,
        &report.aps_anomalies,
        sarif_file.path(),
    ).expect("SARIF generation must succeed");

    // HTML export test
    let html_file = NamedTempFile::new().unwrap();
    generate_html_report(
        &report.deadlocks,
        &report.priority_inversions,
        &report.latency_stats,
        &report.aps_partitions,
        &report.aps_anomalies,
        html_file.path(),
    ).expect("HTML report generation must succeed");
}
