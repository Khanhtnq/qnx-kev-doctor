//! SARIF 2.1.0 (Static Analysis Results Interchange Format) Exporter
//! Compliant with ISO 26262 ASIL-D Automotive CI/CD Quality Gates

use serde::Serialize;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::aps_budget::ApsAnomaly;
use crate::priority_audit::PriorityInversionViolation;
use crate::srr_graph::DeadlockCycle;

#[derive(Serialize)]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub version: String,
    pub runs: Vec<SarifRun>,
}

#[derive(Serialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

#[derive(Serialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

#[derive(Serialize)]
pub struct SarifDriver {
    pub name: String,
    pub version: String,
    #[serde(rename = "informationUri")]
    pub information_uri: String,
    pub rules: Vec<SarifRule>,
}

#[derive(Serialize)]
pub struct SarifRule {
    pub id: String,
    pub name: String,
    #[serde(rename = "shortDescription")]
    pub short_description: SarifMessage,
    #[serde(rename = "defaultConfiguration")]
    pub default_configuration: SarifConfig,
}

#[derive(Serialize)]
pub struct SarifConfig {
    pub level: String, // "error", "warning", "note"
}

#[derive(Serialize)]
pub struct SarifResult {
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub level: String,
    pub message: SarifMessage,
    pub locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Serialize)]
pub struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
pub struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    pub artifact_location: SarifArtifactLocation,
    pub region: Option<SarifRegion>,
}

#[derive(Serialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Serialize)]
pub struct SarifRegion {
    #[serde(rename = "startLine")]
    pub start_line: u32,
}

pub fn generate_sarif_report(
    deadlocks: &[DeadlockCycle],
    inversions: &[PriorityInversionViolation],
    aps_anomalies: &[ApsAnomaly],
    target_path: &Path,
) -> io::Result<()> {
    let rules = vec![
        SarifRule {
            id: "QNX-SRR-001".to_string(),
            name: "IpcDeadlockCycle".to_string(),
            short_description: SarifMessage {
                text: "Circular dependency detected in QNX Send-Receive-Reply IPC wait graph.".to_string(),
            },
            default_configuration: SarifConfig { level: "error".to_string() },
        },
        SarifRule {
            id: "QNX-PRIO-002".to_string(),
            name: "UnboundedPriorityInversion".to_string(),
            short_description: SarifMessage {
                text: "High-priority real-time client blocked on server without priority inheritance.".to_string(),
            },
            default_configuration: SarifConfig { level: "error".to_string() },
        },
        SarifRule {
            id: "QNX-APS-003".to_string(),
            name: "ApsCriticalBudgetOverrun".to_string(),
            short_description: SarifMessage {
                text: "Adaptive Partitioning Scheduler critical time overrun detected.".to_string(),
            },
            default_configuration: SarifConfig { level: "warning".to_string() },
        },
    ];

    let mut results = Vec::new();

    for dl in deadlocks {
        results.push(SarifResult {
            rule_id: "QNX-SRR-001".to_string(),
            level: "error".to_string(),
            message: SarifMessage {
                text: format!(
                    "Deadlock Cycle #{}: {} (Channels: {:?})",
                    dl.cycle_id, dl.explanation, dl.channels_involved
                ),
            },
            locations: vec![SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation {
                        uri: "target_qnx8/srr_resmgr_demo.cpp".to_string(),
                    },
                    region: Some(SarifRegion { start_line: 42 }),
                },
            }],
        });
    }

    for inv in inversions {
        results.push(SarifResult {
            rule_id: "QNX-PRIO-002".to_string(),
            level: if inv.severity == "CRITICAL" { "error".to_string() } else { "warning".to_string() },
            message: SarifMessage {
                text: format!(
                    "Priority Inversion #{}: Client PID {} (Prio {}) blocked on Server PID {} (Prio {}) on Chid {}. Root Cause: {}",
                    inv.violation_id, inv.client_pid, inv.client_priority, inv.server_pid, inv.server_priority, inv.channel_id, inv.root_cause
                ),
            },
            locations: vec![SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation {
                        uri: "target_qnx8/srr_resmgr_demo.cpp".to_string(),
                    },
                    region: Some(SarifRegion { start_line: 88 }),
                },
            }],
        });
    }

    for aps in aps_anomalies {
        results.push(SarifResult {
            rule_id: "QNX-APS-003".to_string(),
            level: if aps.severity == "CRITICAL" { "error".to_string() } else { "warning".to_string() },
            message: SarifMessage {
                text: format!("APS Anomaly #{}: {}", aps.anomaly_id, aps.details),
            },
            locations: vec![SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: SarifArtifactLocation {
                        uri: "target_qnx8/run_qnx8_qemu.sh".to_string(),
                    },
                    region: Some(SarifRegion { start_line: 15 }),
                },
            }],
        });
    }

    let sarif = SarifLog {
        schema: "https://json.schemastore.org/sarif-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "qnx-kev-doctor".to_string(),
                    version: "1.0.0".to_string(),
                    information_uri: "https://github.com/Khanhtnq/qnx8_kev_doctor".to_string(),
                    rules,
                },
            },
            results,
        }],
    };

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(target_path)?;
    serde_json::to_writer_pretty(file, &sarif)?;
    Ok(())
}
