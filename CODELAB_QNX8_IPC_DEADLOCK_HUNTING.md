# Codelab: Hunting IPC Deadlocks & Priority Inversion on QNX Neutrino 8.0

> **Track & Award Eligibility**: QNX Global Hackathon ($50,000 Cash Pool) — *Track #4: Developer Tooling and Host Tools*  
> **Bonus Prize Target**: *Best Codelab ($3,500 USD)* & *Best Use of QNX-specific Technologies ($3,500 USD)*  
> **Author**: Trần Ngọc Quốc Khánh (`Khanhtnq`)  
> **Target OS**: BlackBerry QNX Software Development Platform (SDP) 8.0  
> **Hardware Support**: Raspberry Pi 5 (ARM64 `aarch64`), QEMU `qnx800-x86_64`, NXP i.MX95  

---

## 1. Introduction: The QNX Microkernel IPC Architecture

Unlike monolithic operating systems (such as Linux), BlackBerry QNX Neutrino 8.0 is a **true microkernel architecture**. In QNX Neutrino, everything outside the microkernel core (file systems, network stacks, device drivers, and user applications) executes in separate memory-protected address spaces and interacts via **Synchronous Send-Receive-Reply (`SRR`) Inter-Process Communication (IPC)**.

### The Send-Receive-Reply Contract
1. **Client** calls `MsgSendv(coid, ...)`:
   - Client is placed by the kernel into `STATE_SEND`-blocked until the server executes `MsgReceivev()`.
   - Once received, the client transitions to `STATE_REPLY`-blocked.
   - The client **cannot proceed** until the server explicitly calls `MsgReplyv()`.
2. **Server** calls `MsgReceivev(chid, ...)`:
   - Waits in `STATE_RECEIVE`-blocked if no incoming message is queued.
3. **Server** processes request and calls `MsgReplyv(rcvid, ...)`:
   - Wakes up the client and returns status.

### The Hidden Failure Modes in Multi-Process Architecture
When building complex automotive architectures (such as ISO 26262 ASIL-D ADAS clusters, autonomous braking, and sensor fusion pipelines), three severe microkernel failure modes emerge:

1. **Multi-Process Deadlock Cycle (`A -> B -> C -> A`)**:
   - If Service A sends to Service B, Service B sends to Service C, and Service C sends to Service A, all three processes enter indefinite `STATE_REPLY`-block. The system freezes without throwing a segmentation fault.
2. **Unbounded Priority Inversion (`_NTO_CHF_FIXED_PRIORITY`)**:
   - If a real-time safety critical thread (`prio >= 63`) issues `MsgSend()` to a server channel initialized with `_NTO_CHF_FIXED_PRIORITY`, the server runs at low priority. Intermediate priority processes (e.g., media players or logging daemons) preempt the server, delaying the safety-critical client indefinitely.
3. **Adaptive Partitioning (`APS`) Starvation & Overrun**:
   - Miscalculated partition budgets lead to safety partitions depleting critical CPU budgets.

`qnx-kev-doctor` is purpose-built to solve these issues deterministically from authentic QNX `tracelogger` `.kev` binary traces.

---

## 2. Prerequisites & Environment Setup

### 2.1 QNX SDP 8.0 Installation
Ensure BlackBerry QNX SDP 8.0 is installed:
```bash
source ~/qnx800/qnxsdp-env.sh
qcc -V
# Target compilers: gcc_ntoaarch64_cxx (ARM64) or gcc_ntox86_64_cxx (x86_64)
```

### 2.2 Host Toolchain
- Rust 1.75+ (`cargo`)
- Python 3.10+
- FFmpeg 6.0+

---

## 3. Step 1: Building the QNX 8.0 Native C++20 Microkernel Agent

We provide a production C++20 Resource Manager that exercises channels, pulses, and IPC messages.

Compile for ARM64 (Raspberry Pi 5):
```bash
cd target_qnx8
q++ -Vgcc_ntoaarch64_cxx -std=c++20 -O3 srr_resmgr_demo.cpp -o srr_resmgr_demo -lsocket
```

Or for x86_64 QEMU target:
```bash
q++ -Vgcc_ntox86_64_cxx -std=c++20 -O3 srr_resmgr_demo.cpp -o srr_resmgr_demo -lsocket
```

---

## 4. Step 2: Capturing Kernel Traces with QNX `tracelogger`

On the target QNX Neutrino 8.0 machine, launch `tracelogger` filtering for Communication (`0x02`), Thread State (`0x04`), and APS (`0x08`):

```bash
# Start background kernel trace recording with 128 ring buffers (2 MB)
tracelogger -c -n 128 -k 0x0E -f /tmp/qnx8_audit.kev -d

# Execute the workload
./srr_resmgr_demo

# Stop tracing and flush buffers
tracelogger -k 0
```

Transfer `/tmp/qnx8_audit.kev` to your development host machine via SFTP or SCP:
```bash
scp root@qnx-target:/tmp/qnx8_audit.kev ./traces/
```

---

## 5. Step 3: Running `qnx-kev-doctor` Tarjan SCC Deadlock Analysis

Run the host analyzer:
```bash
cargo run --release -- analyze \
  --input traces/qnx8_audit.kev \
  --out-html docs/index.html \
  --out-sarif reports/qnx_audit.sarif
```

### CLI Diagnostic Output:
```text
================================================================================
       QNX NEUTRINO 8.0 MICROKERNEL TRACE DOCTOR (AUDIT SUMMARY)               
================================================================================
Total Kernel Events Analyzed : 1807

[FAIL] 1 CIRCULAR IPC DEADLOCK CYCLES DETECTED!
  -> Cycle #1: Participants: PID 2002 (TID 1) -> PID 2001 (TID 1) -> PID 1001 (TID 1) -> PID 3001 (TID 1) | Channels: [88, 89, 90, 91]
     Detail: Tarjan SCC detected circular IPC wait-for dependency between 4 threads: PID 2002 TID 1 -> PID 2001 TID 1 -> PID 1001 TID 1 -> PID 3001 TID 1 holding/waiting on channels [88, 89, 90, 91]

[WARN] 1 PRIORITY INVERSIONS DETECTED!
  -> Violation #1: Client PID 1001 (Prio 63) -> Server PID 2001 (Prio 10) on Chid 88
     Root Cause: Channel 88 missing Priority Inheritance! High-prio RT client (prio 63) blocked while server runs at background prio 10. Likely caused by _NTO_CHF_FIXED_PRIORITY flag or non-PI dispatch loop.

[PERF] Channel Roundtrip Latency (Nanoseconds WCET):
  Channel      Samples    Min(ns)    p50(ns)    p90(ns)    p99.9(ns)    Max(ns)   
  Channel 10   200        13000      16207      18607      19807        19807     
  Channel 20   200        13000      16207      18607      19807        19807     
  Channel 30   200        13000      16207      18607      19807        19807     

[APS] Adaptive Partitioning Scheduler (Health Audit):
  Part #1  (Safety_ADAS_Critical  ) | Budget: 50% | MaxUsed: 48% | Overruns:  0 | [HEALTHY]
  Part #3  (Infotainment_Media    ) | Budget: 20% | MaxUsed: 38% | Overruns:  3 | [OVERRUN_DETECTED]
================================================================================
```

---

## 6. Step 4: Investigating the Interactive Dark-Mode Topology Inspector

Open `docs/index.html` in your browser. The tool generates:
1. **Interactive SVG Wait-For Graph**: Directed nodes showing exact PIDs, TIDs, and channel IDs, with circular Tarjan cycles highlighted in pulsing crimson.
2. **Root Cause Analysis Table**: Explaining why each priority inversion occurred and providing the exact line or flag fix (`Remove _NTO_CHF_FIXED_PRIORITY and rely on kernel priority boosting`).
3. **Worst-Case Execution Time (WCET) Quantile Breakdown**: `p50`, `p90`, `p99`, and `p99.9` latency histograms in nanoseconds.
4. **Adaptive Partitioning Budget Bars**: Clear health statuses (`HEALTHY`, `OVERRUN_DETECTED`, `STARVED`).

---

## 7. Step 5: Integrating SARIF 2.1.0 into Automotive ISO 26262 CI/CD Gates

The generated `reports/qnx_audit.sarif` conforms strictly to OASIS SARIF 2.1.0 standards. In your GitHub Actions or GitLab CI pipeline:

```yaml
- name: Run QNX KEV Doctor Quality Gate
  run: |
    qnx-kev-doctor analyze --input build/trace.kev --out-sarif reports/audit.sarif
- name: Upload SARIF to GitHub Code Scanning
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: reports/audit.sarif
```

If any deadlock or critical priority inversion violation occurs, the pipeline fails automatically, preventing defective QNX image builds from flashing into automotive electronic control units (`ECUs`).

---

## 8. Conclusion & Best Practices for QNX 8.0 Development

1. **Never use `_NTO_CHF_FIXED_PRIORITY`** unless the channel explicitly services soft real-time notifications with bounded queues.
2. **Prefer QNX Neutrino Pulses (`MsgSendPulse`)** for asynchronous notifications to prevent circular wait states.
3. **Always monitor WCET p99.9 jitter** during soak tests to detect subtle priority inversions before deployment.
4. **Run `qnx-kev-doctor`** on all soak test traces to guarantee zero deadlocks across all operational modes.
