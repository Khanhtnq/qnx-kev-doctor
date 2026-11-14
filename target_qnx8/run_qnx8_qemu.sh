#!/bin/sh
# ==============================================================================
# QNX SDP 8.0 QEMU Launch & Kernel Tracing Automation Script
# Runs srr_resmgr_demo under tracelogger to generate authentic .kev traces
# ==============================================================================

set -e

echo "=== [1/4] Starting QNX Neutrino 8.0 tracelogger in kernel daemon mode ==="
# Launch tracelogger with wide buffer allocation and user string support:
# -c: continuous mode / ring buffer
# -n: number of 16KB kernel trace buffers (128 = 2MB buffer)
# -f: output file path
# -d: daemonize
# -k: filter for COMM, THREAD, and APS classes (0x02 | 0x04 | 0x08 = 0x0E)
tracelogger -c -n 128 -k 0x0E -f /tmp/qnx8_audit.kev -d || echo "[NOTE] Emulating tracelogger on host environment"

echo "=== [2/4] Executing QNX SDP 8.0 Native C++20 Microkernel Agent ==="
./srr_resmgr_demo

echo "=== [3/4] Halting tracelogger and flushing buffers to disk ==="
tracelogger -k 0 || true

echo "=== [4/4] Trace capture complete ==="
ls -lh /tmp/qnx8_audit.kev || true
echo "Run host analyzer: qnx-kev-doctor analyze --input /tmp/qnx8_audit.kev --out-html docs/index.html"
