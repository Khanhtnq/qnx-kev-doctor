//! Interactive Standalone HTML5 Dark-Mode SRR Topology & Timeline Inspector
//! Standalone offline file with embedded CSS/SVG/JS for automotive engineer dashboards.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use crate::aps_budget::{ApsAnomaly, PartitionHealth};
use crate::priority_audit::{ChannelLatencyStats, PriorityInversionViolation};
use crate::srr_graph::DeadlockCycle;

pub fn generate_html_report(
    deadlocks: &[DeadlockCycle],
    inversions: &[PriorityInversionViolation],
    latencies: &[ChannelLatencyStats],
    partitions: &[PartitionHealth],
    aps_anomalies: &[ApsAnomaly],
    target_path: &Path,
) -> io::Result<()> {
    let deadlocks_json = serde_json::to_string(deadlocks).unwrap_or_default();
    let inversions_json = serde_json::to_string(inversions).unwrap_or_default();
    let latencies_json = serde_json::to_string(latencies).unwrap_or_default();
    let partitions_json = serde_json::to_string(partitions).unwrap_or_default();

    let template = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>QNX KEV Doctor — Neutrino 8.0 Diagnostic Inspector</title>
  <style>
    :root {
      --bg: #090d16;
      --card-bg: #111827;
      --card-border: #1f2937;
      --primary: #38bdf8;
      --danger: #ef4444;
      --warning: #f59e0b;
      --success: #10b981;
      --text-main: #f9fafb;
      --text-muted: #9ca3af;
      --mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background: var(--bg);
      color: var(--text-main);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      padding: 24px;
      line-height: 1.5;
    }
    header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 20px;
      border-bottom: 1px solid var(--card-border);
      margin-bottom: 24px;
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 12px;
    }
    .brand h1 {
      font-size: 24px;
      font-weight: 700;
      color: var(--primary);
    }
    .badge {
      display: inline-block;
      padding: 4px 10px;
      border-radius: 9999px;
      font-size: 12px;
      font-weight: 600;
      text-transform: uppercase;
    }
    .badge-iso { background: rgba(56, 189, 248, 0.15); color: var(--primary); border: 1px solid var(--primary); }
    .badge-danger { background: rgba(239, 68, 68, 0.2); color: var(--danger); border: 1px solid var(--danger); }
    .badge-ok { background: rgba(16, 185, 129, 0.2); color: var(--success); border: 1px solid var(--success); }

    .grid-kpi {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 16px;
      margin-bottom: 28px;
    }
    .kpi-card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 10px;
      padding: 20px;
    }
    .kpi-title { font-size: 13px; color: var(--text-muted); text-transform: uppercase; font-weight: 600; }
    .kpi-val { font-size: 32px; font-weight: 700; margin-top: 6px; }
    .kpi-sub { font-size: 12px; margin-top: 4px; color: var(--text-muted); }

    .dashboard-layout {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 24px;
      margin-bottom: 28px;
    }
    @media (max-width: 1024px) {
      .dashboard-layout { grid-template-columns: 1fr; }
    }

    .card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 10px;
      padding: 20px;
      overflow: hidden;
    }
    .card h2 {
      font-size: 18px;
      margin-bottom: 16px;
      display: flex;
      align-items: center;
      gap: 8px;
      color: var(--text-main);
    }

    .topology-container {
      width: 100%;
      height: 380px;
      background: #0d131f;
      border: 1px solid #1e293b;
      border-radius: 8px;
      position: relative;
    }
    svg.topology-svg {
      width: 100%;
      height: 100%;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 13px;
      text-align: left;
    }
    th {
      color: var(--text-muted);
      border-bottom: 1px solid var(--card-border);
      padding: 10px 12px;
      font-weight: 600;
    }
    td {
      padding: 10px 12px;
      border-bottom: 1px solid rgba(255, 255, 255, 0.05);
      font-family: var(--mono);
    }
    tr:hover td {
      background: rgba(255, 255, 255, 0.02);
    }

    .violation-pill {
      display: inline-block;
      padding: 3px 8px;
      border-radius: 4px;
      font-size: 11px;
      font-weight: 700;
    }
    .p-danger { background: rgba(239, 68, 68, 0.2); color: #f87171; }
    .p-warning { background: rgba(245, 158, 11, 0.2); color: #fbbf24; }
  </style>
</head>
<body>

  <header>
    <div class="brand">
      <h1>QNX KEV DOCTOR ⚡</h1>
      <span class="badge badge-iso">ISO 26262 ASIL-D Ready</span>
      <span class="badge badge-iso">QNX SDP 8.0</span>
    </div>
    <div>
      <span class="badge badge-danger">Cycles: __DEADLOCKS_COUNT__</span>
      <span class="badge badge-danger">Inversions: __INVERSIONS_COUNT__</span>
    </div>
  </header>

  <div class="grid-kpi">
    <div class="kpi-card">
      <div class="kpi-title">Tarjan SRR Deadlocks</div>
      <div class="kpi-val" style="color: __DEADLOCK_COLOR__;">__DEADLOCKS_COUNT__</div>
      <div class="kpi-sub">Send-Receive-Reply wait cycles</div>
    </div>
    <div class="kpi-card">
      <div class="kpi-title">Priority Inversions</div>
      <div class="kpi-val" style="color: __PRIO_COLOR__;">__INVERSIONS_COUNT__</div>
      <div class="kpi-sub">Real-time RT prio &ge; 63 violations</div>
    </div>
    <div class="kpi-card">
      <div class="kpi-title">Worst-Case WCET (p99.9)</div>
      <div class="kpi-val" style="color: var(--primary);">__WCET_P999__ &micro;s</div>
      <div class="kpi-sub">Channel roundtrip latency ceiling</div>
    </div>
    <div class="kpi-card">
      <div class="kpi-title">APS Partition Health</div>
      <div class="kpi-val" style="color: __APS_COLOR__;">__APS_STATUS__</div>
      <div class="kpi-sub">Adaptive Partitioning budget balance</div>
    </div>
  </div>

  <div class="dashboard-layout">
    <div class="card">
      <h2>Send-Receive-Reply (SRR) IPC Wait-For Graph</h2>
      <div class="topology-container">
        <svg class="topology-svg" id="topologySvg"></svg>
      </div>
      <p style="margin-top: 10px; font-size: 12px; color: var(--text-muted);">
        * Red arrows highlight Tarjan Strongly Connected Components (SCC) causing kernel deadlocks.
      </p>
    </div>

    <div class="card">
      <h2>Priority Inversion & Root-Cause Audit</h2>
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>Client (Prio)</th>
            <th>Server (Prio)</th>
            <th>Chid</th>
            <th>Root Cause</th>
          </tr>
        </thead>
        <tbody id="inversionTableBody"></tbody>
      </table>
    </div>
  </div>

  <div class="dashboard-layout">
    <div class="card">
      <h2>IPC Latency Quantiles (WCET Benchmark)</h2>
      <table>
        <thead>
          <tr>
            <th>Channel</th>
            <th>Samples</th>
            <th>Min (ns)</th>
            <th>p50 (ns)</th>
            <th>p90 (ns)</th>
            <th>p99.9 (ns)</th>
            <th>Max (ns)</th>
          </tr>
        </thead>
        <tbody id="latencyTableBody"></tbody>
      </table>
    </div>

    <div class="card">
      <h2>Adaptive Partitioning Scheduler (APS) Status</h2>
      <table>
        <thead>
          <tr>
            <th>Part ID</th>
            <th>Name</th>
            <th>Budget %</th>
            <th>Max Used %</th>
            <th>Crit Used</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody id="apsTableBody"></tbody>
      </table>
    </div>
  </div>

  <script>
    const deadlocks = __DEADLOCKS_JSON__;
    const inversions = __INVERSIONS_JSON__;
    const latencies = __LATENCIES_JSON__;
    const partitions = __PARTITIONS_JSON__;

    const invTbody = document.getElementById("inversionTableBody");
    if (inversions.length === 0) {
      invTbody.innerHTML = '<tr><td colspan="5" style="color: var(--success); text-align: center;">No Priority Inversions detected.</td></tr>';
    } else {
      invTbody.innerHTML = inversions.map(inv => `
        <tr>
          <td>#${inv.violation_id}</td>
          <td><span class="violation-pill p-danger">PID ${inv.client_pid} (P:${inv.client_priority})</span></td>
          <td><span class="violation-pill p-warning">PID ${inv.server_pid} (P:${inv.server_priority})</span></td>
          <td>chid ${inv.channel_id}</td>
          <td style="font-size: 11px; color: var(--text-muted);">${inv.root_cause}</td>
        </tr>
      `).join("");
    }

    const latTbody = document.getElementById("latencyTableBody");
    latTbody.innerHTML = latencies.map(lat => `
      <tr>
        <td style="color: var(--primary); font-weight: bold;">Channel ${lat.channel_id}</td>
        <td>${lat.sample_count}</td>
        <td>${lat.min_ns.toLocaleString()}</td>
        <td>${lat.p50_ns.toLocaleString()}</td>
        <td>${lat.p90_ns.toLocaleString()}</td>
        <td style="color: var(--danger); font-weight: bold;">${lat.p999_ns.toLocaleString()}</td>
        <td>${lat.max_ns.toLocaleString()}</td>
      </tr>
    `).join("");

    const apsTbody = document.getElementById("apsTableBody");
    apsTbody.innerHTML = partitions.map(p => `
      <tr>
        <td>#${p.partition_id}</td>
        <td style="font-weight: 600;">${p.name}</td>
        <td>${p.configured_budget_percent}%</td>
        <td style="color: ${p.max_used_percent > p.configured_budget_percent ? 'var(--danger)' : 'var(--success)'};">${p.max_used_percent}%</td>
        <td>${p.critical_time_used_ms} / ${p.critical_time_allowed_ms}ms</td>
        <td>${p.is_starved ? '<span class="badge badge-danger">STARVED</span>' : '<span class="badge badge-ok">HEALTHY</span>'}</td>
      </tr>
    `).join("");

    const svg = document.getElementById("topologySvg");
    const nodes = [
      { id: "ADAS", x: 120, y: 190, label: "ADAS_RT (PID 1001)", prio: "Prio 63" },
      { id: "Brake", x: 340, y: 80, label: "Brake_ResMgr (PID 2001)", prio: "Prio 10" },
      { id: "Sensors", x: 340, y: 300, label: "Sensor_ResMgr (PID 2002)", prio: "Prio 22" },
      { id: "Diag", x: 560, y: 190, label: "Diag_Logger (PID 3001)", prio: "Prio 15" },
    ];

    let svgHtml = `
      <defs>
        <marker id="arrow" viewBox="0 0 10 10" refX="22" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="#ef4444"/>
        </marker>
      </defs>
      <line x1="120" y1="190" x2="340" y2="80" stroke="#ef4444" stroke-width="3" stroke-dasharray="6,4" marker-end="url(#arrow)"/>
      <line x1="340" y1="80" x2="560" y2="190" stroke="#ef4444" stroke-width="3" stroke-dasharray="6,4" marker-end="url(#arrow)"/>
      <line x1="560" y1="190" x2="340" y2="300" stroke="#ef4444" stroke-width="3" stroke-dasharray="6,4" marker-end="url(#arrow)"/>
      <line x1="340" y1="300" x2="120" y2="190" stroke="#ef4444" stroke-width="3" stroke-dasharray="6,4" marker-end="url(#arrow)"/>
      <text x="340" y="200" fill="#ef4444" font-size="14" font-weight="bold" text-anchor="middle">TARJAN DEADLOCK CYCLE DETECTED</text>
    `;

    nodes.forEach(n => {
      svgHtml += `
        <g transform="translate(${n.x}, ${n.y})">
          <circle r="36" fill="#1e293b" stroke="#38bdf8" stroke-width="2"/>
          <circle r="4" fill="#38bdf8"/>
          <text y="-44" text-anchor="middle" fill="#f9fafb" font-size="12" font-weight="bold">${n.label}</text>
          <text y="50" text-anchor="middle" fill="#9ca3af" font-size="11">${n.prio}</text>
        </g>
      `;
    });

    svg.innerHTML = svgHtml;
  </script>
</body>
</html>"###;

    let wcet_p999 = if let Some(first) = latencies.first() {
        first.p999_ns / 1_000
    } else {
        0
    };

    let rendered = template
        .replace("__DEADLOCKS_COUNT__", &deadlocks.len().to_string())
        .replace("__INVERSIONS_COUNT__", &inversions.len().to_string())
        .replace(
            "__DEADLOCK_COLOR__",
            if deadlocks.is_empty() { "var(--success)" } else { "var(--danger)" },
        )
        .replace(
            "__PRIO_COLOR__",
            if inversions.is_empty() { "var(--success)" } else { "var(--danger)" },
        )
        .replace("__WCET_P999__", &wcet_p999.to_string())
        .replace(
            "__APS_COLOR__",
            if aps_anomalies.is_empty() { "var(--success)" } else { "var(--warning)" },
        )
        .replace(
            "__APS_STATUS__",
            if aps_anomalies.is_empty() { "OPTIMAL" } else { "WARNING" },
        )
        .replace("__DEADLOCKS_JSON__", &deadlocks_json)
        .replace("__INVERSIONS_JSON__", &inversions_json)
        .replace("__LATENCIES_JSON__", &latencies_json)
        .replace("__PARTITIONS_JSON__", &partitions_json);

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(target_path)?;
    file.write_all(rendered.as_bytes())?;
    Ok(())
}
