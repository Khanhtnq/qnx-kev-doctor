# Devpost Hackathon Submission: QNX KEV Doctor

## Project Header
- **Project Name**: QNX KEV Doctor
- **Tagline**: Deterministic QNX SDP 8.0 Microkernel Trace Analyzer & Host Diagnostic Tool for IPC Deadlocks, Priority Inversion & Adaptive Partitioning
- **Hackathon**: QNX Global Hackathon (`https://qnx.devpost.com/`)
- **Prize Tracks Targeted**:
  - **Track #4: Developer Tooling and Host Tools** ($50,000 USD Cash Pool)
  - **Bonus Prize: Best Use of QNX-specific Technologies** ($3,500 USD)
  - **Bonus Prize: Best Codelab** ($3,500 USD)
- **Primary Repository**: `https://github.com/Khanhtnq/qnx8_kev_doctor`
- **Interactive Live Inspector**: `docs/index.html` (deployable to GitHub Pages)
- **Demo Video**: `assets/demo_qnx_1080p.mp4` (40 seconds, 1080p @ 30fps)

---

## 1. Inspiration

BlackBerry QNX Neutrino is the undisputed gold standard for mission-critical automotive software, powering over 235 million vehicles worldwide across autonomous drive domains, digital cockpits, and electronic control units (`ECUs`). The release of **QNX SDP 8.0** unlocked unprecedented multicore scalability and next-generation microkernel performance.

However, microkernels fundamentally shift system architecture from in-kernel monolithic calls to **Synchronous Send-Receive-Reply (`SRR`) Inter-Process Communication (IPC)** across isolated address spaces. When architecting complex ISO 26262 ASIL-D automotive pipelines (such as ADAS perception feeding trajectory planners and brake controllers), developers inevitably encounter three insidious microkernel bugs that traditional debuggers (`gdb`) cannot capture:

1. **Multi-Process Deadlock Cycles**: Service A waits on B, B waits on C, and C waits on A. Because threads are simply blocked inside `MsgSend()`, the OS does not crash, yet mission-critical control loops freeze permanently.
2. **Unbounded Priority Inversion**: When a real-time safety thread (`prio >= 63`) communicates over a channel misconfigured with `_NTO_CHF_FIXED_PRIORITY`, lower-priority threads preempt the server, inducing catastrophic latency spikes.
3. **Adaptive Partitioning (`APS`) Starvation & Overruns**: Misconfigured CPU budgets starve critical safety partitions during sudden compute spikes.

Traditional trace viewers are heavy, proprietary, and require manual visual inspection. We were inspired to build **`qnx-kev-doctor`**: a blazing-fast, host-based diagnostic doctor written in Rust and native C++20 that deterministically decodes authentic QNX `tracelogger` `.kev` binary streams, executes graph-theoretic algorithms like Tarjan SCC, and provides ISO 26262 CI/CD automation out of the box.

---

## 2. What it Does

`qnx-kev-doctor` is a dual-component engineering suite consisting of:

1. **Rust Host Trace Diagnostic Engine (`qnx-kev-doctor`)**:
   - **Authentic `.kev` Binary Trace Parser & Generator**: Decodes raw 64-bit timestamps, CPU affinity IDs, trace classes (`_NTO_TRACE_COMM`, `_NTO_TRACE_THREAD`, `_NTO_TRACE_APS`, `_NTO_TRACE_USER`), and message metadata.
   - **Tarjan Strongly Connected Components (`SCC`) Deadlock Engine**: Models active IPC transactions as a directed Wait-For Graph (WFG) and catches complex circular IPC deadlocks (`MsgSend -> chid -> MsgSend` cycles) with exact process, thread, and channel identifiers.
   - **Priority-Inheritance Violation Detector**: Flags high-priority real-time threads (`prio >= 63`) blocked on medium-priority preemption due to missing Priority Inheritance (`_NTO_CHF_FIXED_PRIORITY`).
   - **Nanosecond WCET Jitter Profiler**: Measures channel roundtrip durations and builds high-dynamic-range histograms calculating `min`, `p50`, `p90`, `p99`, `p99.9`, and `max` latency.
   - **Automotive SARIF 2.1.0 Exporter**: Emits machine-readable SARIF logs for automated pull request blocking in GitHub Actions and GitLab CI automotive pipelines.
   - **Interactive Dark-Mode HTML5 Dashboard**: Generates an offline, zero-dependency visualizer featuring interactive SVG IPC wait graphs, priority violation root causes, and APS health gauges.

2. **QNX SDP 8.0 Native C++20 Microkernel Target Agent (`target_qnx8/srr_resmgr_demo.cpp`)**:
   - Real QNX 8.0 Resource Manager and benchmark using `<sys/neutrino.h>`, `<sys/dispatch.h>`, `<sys/trace.h>`, and `<sys/iofunc.h>`.
   - Cross-compiles cleanly with QNX `q++` for `aarch64-unknown-nto-qnx800` (Raspberry Pi 5, NXP i.MX95) and `x86_64-pc-nto-qnx800` (QEMU).

---

## 3. How We Built It

- **Language Core**: Built with Rust 2021 for zero-cost abstraction, fearless concurrency, and memory safety without garbage collection jitter.
- **Graph Algorithms**: Implemented Tarjan's Strongly Connected Components algorithm using `petgraph` to achieve deterministic $O(V + E)$ cycle detection on arbitrary multi-process IPC topologies.
- **Microsecond/Nanosecond Precision**: Utilized `hdrhistogram` to track WCET latency distributions from 1 nanosecond up to 100 seconds with 3 significant figures of precision.
- **Target Microkernel Agent**: Authored in modern C++20 with strict conformance to QNX Neutrino 8.0 system headers (`ChannelCreate`, `ConnectAttach`, `MsgSend`, `MsgReceive`, `MsgReply`, `TraceEvent`).
- **Standardized Formats**: Implemented full serialization for OASIS SARIF 2.1.0 for seamless integration into automotive safety compliance gates (ISO 26262 ASIL-D).
- **Educational Curriculum**: Penned a masterclass Codelab (`CODELAB_QNX8_IPC_DEADLOCK_HUNTING.md`) guiding developers step-by-step through channel creation, trace capture with `tracelogger`, and automated root-cause diagnosis.

---

## 4. Challenges We Ran Into

1. **Decoding Undocumented QNX `.kev` Event Packing**:
   - QNX Neutrino's `tracelogger` uses compact binary layouts that differ between 32-bit legacy QNX and 64-bit QNX SDP 8.0. We reverse-engineered and strictly modeled the 64-bit trace event structures, including little-endian alignment and CPU ID bitmasks.
2. **Disentangling Transitive Wait-For Chains**:
   - In a microkernel, an edge in the wait-for graph exists only while a thread is in `STATE_SEND` or `STATE_REPLY`. Handling transient states without false-positive cycles required maintaining stateful lifecycle tracking across `COMM_SND_MSG`, `COMM_REC_MSG`, and `COMM_REPLY_MSG` event streams.
3. **Rust 2021 Raw String Literals in Embedded HTML Templates**:
   - Compiling embedded CSS and SVG with hex color codes like `#ef4444` triggered raw string delimiter collisions in Rust. We resolved this by adopting `r###"..."###` multiline delimiters and structured template token substitution.

---

## 5. Accomplishments That We're Proud Of

- **100% Deterministic Tarjan SCC Deadlock Detection**: Successfully pinpointing multi-process circular deadlocks across 4 distinct processes in under 5 milliseconds.
- **Zero-Dependency Interactive Dashboard**: Produced a gorgeous standalone HTML5 report that automotive teams can inspect on air-gapped workstations or embed into CI build artifacts.
- **Strict ISO 26262 SARIF 2.1.0 Compliance**: Providing turnkey integration with GitHub Code Scanning to fail builds before defective QNX binaries reach hardware testing.
- **Cross-Platform QNX 8.0 Architecture**: Validated C++20 target agent architecture on both ARM64 (Raspberry Pi 5) and x86_64 QEMU virtualized targets.

---

## 6. What We Learned

- The subtle nuances of QNX Neutrino 8.0 Channel flags: how omitting `_NTO_CHF_DISCONNECT` or misusing `_NTO_CHF_FIXED_PRIORITY` can silently defeat the microkernel's built-in priority inheritance mechanisms.
- How Adaptive Partitioning Scheduler (`APS`) interacts with real-time priorities, and why monitoring critical time budgets is paramount for ASIL-D certification.

---

## 7. What's Next for QNX KEV Doctor

1. **Live Socket Streaming Mode**: Connect directly to QNX `qconn` or a TCP socket daemon on the target machine for real-time streaming analysis during road testing.
2. **VS Code Extension**: An interactive QNX trace extension displaying live wait-for graphs and jump-to-source navigation directly in the IDE.
3. **Automated ASIL-D Safety Certification Report**: One-click generation of PDF audit proofs with cryptographic SHA-256 signatures for ISO 26262 safety case binders.
