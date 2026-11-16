# QNX KEV Doctor ⚡ — Deterministic QNX SDP 8.0 Microkernel Trace Analyzer

[![QNX SDP 8.0](https://img.shields.io/badge/QNX-SDP%208.0%20Neutrino-0ea5e9.svg)](https://qnx.devpost.com/)
[![Rust 2021](https://img.shields.io/badge/Host%20Engine-Rust%202021-f97316.svg)](Cargo.toml)
[![ISO 26262 SARIF](https://img.shields.io/badge/CI%2FCD-SARIF%202.1.0-10b981.svg)](reports/qnx_audit.sarif)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**QNX KEV Doctor** (`qnx-kev-doctor`) is a deterministic microkernel `.kev` binary trace analyzer and native C++20 target benchmark suite built for **BlackBerry QNX SDP 8.0**. It catches circular Synchronous Send-Receive-Reply (`SRR`) IPC deadlocks, unbounded priority inversions, and Adaptive Partitioning (`APS`) budget starvation before defective automotive binaries reach ECU hardware.

- **Interactive Live Trace Inspector**: [https://khanhtnq.github.io/qnx-kev-doctor/](https://khanhtnq.github.io/qnx-kev-doctor/)
- **Step-by-Step QNX 8.0 Codelab**: [`CODELAB_QNX8_IPC_DEADLOCK_HUNTING.md`](CODELAB_QNX8_IPC_DEADLOCK_HUNTING.md)
- **Devpost Submission Copy**: [`DEVPOST_QNX_SUBMISSION.md`](DEVPOST_QNX_SUBMISSION.md)

---

## 🛠️ Architecture

1. **Rust Host Diagnostic Engine (`src/`)**:
   - **`.kev` Binary Parser (`src/kev_parser.rs`)**: Parses 64-bit QNX Neutrino `tracelogger` event streams (`_NTO_TRACE_COMM`, `_NTO_TRACE_THREAD`, `_NTO_TRACE_APS`).
   - **Tarjan SCC Deadlock Detector (`src/srr_graph.rs`)**: Constructs directed thread/channel Wait-For Graphs and pinpoints multi-process `MsgSend` cycles in $O(V + E)$ time.
   - **Priority Inheritance Auditor (`src/priority_audit.rs`)**: Detects high-priority real-time clients (`prio >= 63`) blocked on `_NTO_CHF_FIXED_PRIORITY` channels.
   - **HdrHistogram WCET & SARIF 2.1.0 Exporter (`src/sarif.rs`, `src/html_report.rs`)**: Emits nanosecond `p50`/`p99`/`p99.9` latency metrics, offline dark-mode SVG dashboards, and GitHub Code Scanning SARIF gates.
2. **QNX SDP 8.0 Target Resource Manager (`target_qnx8/srr_resmgr_demo.cpp`)**:
   - Native C++20 microkernel target agent cross-compiling with `q++` for `aarch64-unknown-nto-qnx800` (Raspberry Pi 5 / i.MX95) and `x86_64-pc-nto-qnx800` (QEMU).

---

## 🚀 Quick Start

```bash
# Run automated Rust integration tests
cargo test

# Generate a synthetic multi-process QNX 8.0 .kev trace & run full audit
cargo run --release -- demo \
  --out-kev reports/demo_synthetic.kev \
  --out-html docs/index.html \
  --out-sarif reports/qnx_audit.sarif
```
