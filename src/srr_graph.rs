//! QNX Neutrino 8.0 Send-Receive-Reply (SRR) IPC Wait-For Graph & Tarjan SCC Cycle Detector
//!
//! In QNX Neutrino microkernel architecture:
//! - Thread A issues MsgSend to a Channel owned by Thread B (process X).
//! - Thread A is placed into STATE_SEND-blocked or STATE_REPLY-blocked waiting on B.
//! - If Thread B (or a downstream server C) synchronously calls MsgSend back to A or
//!   forms a circular dependency chain (A -> B -> C -> A), a hard microkernel deadlock occurs.
//!
//! This module tracks active SRR transactions from .kev events, constructs the directed
//! Wait-For Graph (WFG), and executes Tarjan's Strongly Connected Components (SCC) algorithm
//! to identify deadlocks and pinpoint the exact offending channels, coids, and thread IDs.

use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::kev_parser::{
    COMM_REPLY_MSG, COMM_SND_MSG, KevEvent, KevPayload,
    QNX_TRACE_CLASS_COMM, QNX_TRACE_CLASS_THREAD, TH_RECEIVE, TH_REPLY, TH_SEND,
};

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessThread {
    pub pid: u32,
    pub tid: u32,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrrEdge {
    pub channel_id: u32,
    pub coid: u32,
    pub send_timestamp_ns: u64,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadlockCycle {
    pub cycle_id: usize,
    pub participants: Vec<ProcessThread>,
    pub channels_involved: Vec<u32>,
    pub timestamp_ns: u64,
    pub explanation: String,
}

#[derive(Debug, Default)]
pub struct SrrWaitGraph {
    /// Graph of waiting threads: edge A -> B means A is blocked waiting for B to reply
    graph: DiGraph<ProcessThread, SrrEdge>,
    node_map: HashMap<ProcessThread, NodeIndex>,
    reverse_node_map: HashMap<NodeIndex, ProcessThread>,
    /// Active pending sends: key = (sender_pid, sender_tid) -> (target_pid, target_tid, edge)
    pending_sends: HashMap<ProcessThread, (ProcessThread, SrrEdge)>,
    /// Channel ownership map: channel_id -> (owner_pid, owner_tid)
    channel_owners: HashMap<u32, ProcessThread>,
}

impl SrrWaitGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_channel_owner(&mut self, channel_id: u32, pid: u32, tid: u32) {
        let pt = ProcessThread { pid, tid, name: None };
        self.channel_owners.insert(channel_id, pt);
    }

    fn get_or_insert_node(&mut self, pt: ProcessThread) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(&pt) {
            idx
        } else {
            let idx = self.graph.add_node(pt.clone());
            self.node_map.insert(pt.clone(), idx);
            self.reverse_node_map.insert(idx, pt);
            idx
        }
    }

    /// Process a stream of QNX kernel events and maintain active wait edges
    pub fn feed_event(&mut self, ev: &KevEvent) {
        match ev.class {
            QNX_TRACE_CLASS_COMM => match &ev.payload {
                KevPayload::CommMsg {
                    pid,
                    tid,
                    chid,
                    coid,
                    target_pid,
                    target_tid,
                    priority,
                    ..
                } => {
                    let sender = ProcessThread {
                        pid: *pid,
                        tid: *tid,
                        name: None,
                    };

                    // Determine receiver thread: either explicit target or registered channel owner
                    let target = if *target_pid != 0 {
                        ProcessThread {
                            pid: *target_pid,
                            tid: *target_tid,
                            name: None,
                        }
                    } else if let Some(owner) = self.channel_owners.get(chid) {
                        owner.clone()
                    } else {
                        ProcessThread {
                            pid: *chid,
                            tid: 1,
                            name: Some(format!("ChannelOwner_ch{}", chid)),
                        }
                    };

                    if ev.event_code == COMM_SND_MSG {
                        let edge = SrrEdge {
                            channel_id: *chid,
                            coid: *coid,
                            send_timestamp_ns: ev.timestamp_ns,
                            priority: *priority,
                        };
                        self.pending_sends.insert(sender.clone(), (target.clone(), edge));
                        self.rebuild_graph();
                    } else if ev.event_code == COMM_REPLY_MSG {
                        // Reply received, unblock sender
                        self.pending_sends.remove(&sender);
                        self.rebuild_graph();
                    }
                }
                _ => {}
            },
            QNX_TRACE_CLASS_THREAD => match &ev.payload {
                KevPayload::ThreadState {
                    pid,
                    tid,
                    channel_id,
                    blocked_on_pid,
                    blocked_on_tid,
                    priority,
                    ..
                } => {
                    let sender = ProcessThread {
                        pid: *pid,
                        tid: *tid,
                        name: None,
                    };
                    if ev.event_code == TH_SEND || ev.event_code == TH_REPLY {
                        let target = if *blocked_on_pid != 0 {
                            ProcessThread {
                                pid: *blocked_on_pid,
                                tid: *blocked_on_tid,
                                name: None,
                            }
                        } else if let Some(owner) = self.channel_owners.get(channel_id) {
                            owner.clone()
                        } else {
                            ProcessThread {
                                pid: *channel_id,
                                tid: 1,
                                name: Some(format!("Channel_{}", channel_id)),
                            }
                        };
                        let edge = SrrEdge {
                            channel_id: *channel_id,
                            coid: 0,
                            send_timestamp_ns: ev.timestamp_ns,
                            priority: *priority,
                        };
                        self.pending_sends.insert(sender, (target, edge));
                        self.rebuild_graph();
                    } else if ev.event_code == TH_RECEIVE || ev.event_code == 0 {
                        // Unblocked
                        self.pending_sends.remove(&sender);
                        self.rebuild_graph();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn rebuild_graph(&mut self) {
        self.graph.clear();
        self.node_map.clear();
        self.reverse_node_map.clear();

        let pending_items: Vec<(ProcessThread, ProcessThread, SrrEdge)> = self
            .pending_sends
            .iter()
            .map(|(sender, (target, edge))| (sender.clone(), target.clone(), edge.clone()))
            .collect();

        for (sender, target, edge) in pending_items {
            let u = self.get_or_insert_node(sender);
            let v = self.get_or_insert_node(target);
            self.graph.add_edge(u, v, edge);
        }
    }

    /// Detect cycles using Tarjan's Strongly Connected Components (SCC)
    pub fn detect_deadlocks(&self, current_ts_ns: u64) -> Vec<DeadlockCycle> {
        let sccs = tarjan_scc(&self.graph);
        let mut deadlocks = Vec::new();
        let mut cycle_id = 1;

        for scc in sccs {
            // An SCC with > 1 node or a node with a self-loop is a deadlock cycle
            let is_cycle = if scc.len() > 1 {
                true
            } else if scc.len() == 1 {
                // Check for self-loop
                let node = scc[0];
                self.graph.contains_edge(node, node)
            } else {
                false
            };

            if is_cycle {
                let mut participants = Vec::new();
                let mut channels_involved = HashSet::new();

                for &node_idx in &scc {
                    if let Some(pt) = self.reverse_node_map.get(&node_idx) {
                        participants.push(pt.clone());
                    }
                }

                // Collect channels in this cycle
                for &u in &scc {
                    for &v in &scc {
                        if let Some(edge_idx) = self.graph.find_edge(u, v) {
                            if let Some(edge) = self.graph.edge_weight(edge_idx) {
                                channels_involved.insert(edge.channel_id);
                            }
                        }
                    }
                }

                let mut sorted_channels: Vec<u32> = channels_involved.into_iter().collect();
                sorted_channels.sort_unstable();

                let explanation = format!(
                    "Tarjan SCC detected circular IPC wait-for dependency between {} threads: {} holding/waiting on channels {:?}",
                    participants.len(),
                    participants
                        .iter()
                        .map(|p| format!("PID {} TID {}", p.pid, p.tid))
                        .collect::<Vec<_>>()
                        .join(" -> "),
                    sorted_channels
                );

                deadlocks.push(DeadlockCycle {
                    cycle_id,
                    participants,
                    channels_involved: sorted_channels,
                    timestamp_ns: current_ts_ns,
                    explanation,
                });
                cycle_id += 1;
            }
        }

        deadlocks
    }

    pub fn active_wait_count(&self) -> usize {
        self.pending_sends.len()
    }
}
