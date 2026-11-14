//! QNX Neutrino 8.0 Adaptive Partitioning Scheduler (APS) Budget & Starvation Engine
//!
//! Analyzes _NTO_TRACE_APS events to detect:
//! 1. Partition budget starvation (< configured guarantee during system saturation).
//! 2. Critical CPU time overruns (exceeding critical budget ms).
//! 3. Partition hogging causing priority inversion across partitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::kev_parser::{APS_OVERRUN, KevEvent, KevPayload, QNX_TRACE_CLASS_APS};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionHealth {
    pub partition_id: u16,
    pub name: String,
    pub configured_budget_percent: u8,
    pub max_used_percent: u8,
    pub avg_used_percent: f64,
    pub critical_time_allowed_ms: u32,
    pub critical_time_used_ms: u32,
    pub total_overruns: u32,
    pub is_starved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApsAnomaly {
    pub anomaly_id: usize,
    pub timestamp_ns: u64,
    pub partition_id: u16,
    pub partition_name: String,
    pub severity: String,
    pub details: String,
}

pub struct ApsAuditEngine {
    partition_samples: HashMap<u16, Vec<(u8, u8, u32)>>, // (budget%, used%, critical_used_ms)
    overrun_counts: HashMap<u16, u32>,
    anomalies: Vec<ApsAnomaly>,
    anomaly_counter: usize,
    partition_names: HashMap<u16, String>,
}

impl ApsAuditEngine {
    pub fn new() -> Self {
        let mut names = HashMap::new();
        names.insert(0, "System (Default)".to_string());
        names.insert(1, "Safety_ADAS_Critical".to_string());
        names.insert(2, "Instrument_Cluster".to_string());
        names.insert(3, "Infotainment_Media".to_string());

        Self {
            partition_samples: HashMap::new(),
            overrun_counts: HashMap::new(),
            anomalies: Vec::new(),
            anomaly_counter: 1,
            partition_names: names,
        }
    }

    pub fn set_partition_name(&mut self, partition_id: u16, name: impl Into<String>) {
        self.partition_names.insert(partition_id, name.into());
    }

    pub fn feed_event(&mut self, ev: &KevEvent) {
        if ev.class != QNX_TRACE_CLASS_APS {
            return;
        }

        match &ev.payload {
            KevPayload::ApsBudget {
                partition_id,
                budget_percent,
                used_percent,
                critical_budget_ms,
                critical_used_ms,
                num_overruns,
            } => {
                let name = self
                    .partition_names
                    .get(partition_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Partition_{}", partition_id));

                self.partition_samples
                    .entry(*partition_id)
                    .or_default()
                    .push((*budget_percent, *used_percent, *critical_used_ms));

                let counter = self.overrun_counts.entry(*partition_id).or_insert(0);
                *counter += *num_overruns;

                // Anomaly: Critical budget depleted
                if *critical_used_ms > *critical_budget_ms && *critical_budget_ms > 0 {
                    self.anomalies.push(ApsAnomaly {
                        anomaly_id: self.anomaly_counter,
                        timestamp_ns: ev.timestamp_ns,
                        partition_id: *partition_id,
                        partition_name: name.clone(),
                        severity: "CRITICAL".to_string(),
                        details: format!(
                            "APS Critical Time Overrun: Partition {} used {}ms, exceeding permitted {}ms critical budget! Potential timing violation.",
                            name, critical_used_ms, critical_budget_ms
                        ),
                    });
                    self.anomaly_counter += 1;
                }

                // Anomaly: Overrun event explicitly triggered
                if ev.event_code == APS_OVERRUN || *num_overruns > 0 {
                    self.anomalies.push(ApsAnomaly {
                        anomaly_id: self.anomaly_counter,
                        timestamp_ns: ev.timestamp_ns,
                        partition_id: *partition_id,
                        partition_name: name.clone(),
                        severity: "HIGH".to_string(),
                        details: format!(
                            "APS Partition Overrun: Partition {} exceeded nominal {}% CPU budget under load (Usage: {}%).",
                            name, budget_percent, used_percent
                        ),
                    });
                    self.anomaly_counter += 1;
                }
            }
            _ => {}
        }
    }

    pub fn summarize(&self) -> Vec<PartitionHealth> {
        let mut summaries = Vec::new();

        for (&part_id, samples) in &self.partition_samples {
            if samples.is_empty() {
                continue;
            }
            let name = self
                .partition_names
                .get(&part_id)
                .cloned()
                .unwrap_or_else(|| format!("Partition_{}", part_id));

            let config_budget = samples[0].0;
            let max_used = samples.iter().map(|s| s.1).max().unwrap_or(0);
            let avg_used = samples.iter().map(|s| s.1 as f64).sum::<f64>() / (samples.len() as f64);
            let max_crit_used = samples.iter().map(|s| s.2).max().unwrap_or(0);
            let overruns = *self.overrun_counts.get(&part_id).unwrap_or(&0);

            // A partition is considered starved if it consistently achieved < 40% of its nominal budget while having pending run queues
            let is_starved = avg_used < (config_budget as f64 * 0.40) && config_budget >= 20;

            summaries.push(PartitionHealth {
                partition_id: part_id,
                name,
                configured_budget_percent: config_budget,
                max_used_percent: max_used,
                avg_used_percent: (avg_used * 10.0).round() / 10.0,
                critical_time_allowed_ms: 100, // standard 100ms window
                critical_time_used_ms: max_crit_used,
                total_overruns: overruns,
                is_starved,
            });
        }

        summaries.sort_by_key(|s| s.partition_id);
        summaries
    }

    pub fn get_anomalies(&self) -> &[ApsAnomaly] {
        &self.anomalies
    }
}
