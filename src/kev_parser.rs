//! QNX Neutrino 8.0 Kernel Event Trace (.kev) Binary Parser & Generator
//! Models authentic tracelogger buffer formats, event classes, and payload structures.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{self, Cursor, Read, Write};

/// QNX Neutrino Trace Classes (_NTO_TRACE_*)
pub const QNX_TRACE_CLASS_SYSTEM: u8 = 0x01;
pub const QNX_TRACE_CLASS_COMM: u8 = 0x02;
pub const QNX_TRACE_CLASS_THREAD: u8 = 0x04;
pub const QNX_TRACE_CLASS_APS: u8 = 0x08;
pub const QNX_TRACE_CLASS_USER: u8 = 0x20;

/// QNX Neutrino Communication Event Codes (_NTO_TRACE_COMM_*)
pub const COMM_SND_MSG: u8 = 0x01;
pub const COMM_REC_MSG: u8 = 0x02;
pub const COMM_REPLY_MSG: u8 = 0x03;
pub const COMM_SND_PULSE: u8 = 0x04;

/// QNX Neutrino Thread State Event Codes (_NTO_TRACE_TH*)
pub const TH_RUNNING: u8 = 0x01;
pub const TH_READY: u8 = 0x02;
pub const TH_SEND: u8 = 0x03;
pub const TH_RECEIVE: u8 = 0x04;
pub const TH_REPLY: u8 = 0x05;
pub const TH_MUTEX: u8 = 0x06;

/// QNX Neutrino Adaptive Partitioning Codes (_NTO_TRACE_APS_*)
pub const APS_BUDGET_SAMPLE: u8 = 0x01;
pub const APS_OVERRUN: u8 = 0x02;

/// QNX .kev Magic Header: "QNXKEV80" (8 bytes)
pub const KEV_MAGIC: [u8; 8] = *b"QNXKEV80";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KevHeader {
    pub magic: [u8; 8],
    pub major_version: u16,
    pub minor_version: u16,
    pub endian: u16,      // 0x0100 for Little-Endian
    pub cpu_arch: u16,    // 0x01 = aarch64, 0x02 = x86_64
    pub num_cpus: u32,
    pub timestamp_freq_hz: u64, // e.g., 1_000_000_000 (1 GHz TSC / cycle counter)
    pub total_events: u64,
}

impl Default for KevHeader {
    fn default() -> Self {
        Self {
            magic: KEV_MAGIC,
            major_version: 8,
            minor_version: 0,
            endian: 0x0100,
            cpu_arch: 0x01, // aarch64 default
            num_cpus: 4,
            timestamp_freq_hz: 1_000_000_000,
            total_events: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KevPayload {
    CommMsg {
        pid: u32,
        tid: u32,
        chid: u32,
        coid: u32,
        target_pid: u32,
        target_tid: u32,
        msg_size: u32,
        priority: u8,
    },
    ThreadState {
        pid: u32,
        tid: u32,
        priority: u8,
        inherited_priority: u8,
        channel_id: u32,
        blocked_on_pid: u32,
        blocked_on_tid: u32,
    },
    ApsBudget {
        partition_id: u16,
        budget_percent: u8,
        used_percent: u8,
        critical_budget_ms: u32,
        critical_used_ms: u32,
        num_overruns: u32,
    },
    UserString {
        pid: u32,
        tid: u32,
        event_code: u16,
        text: String,
    },
    RawBytes(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KevEvent {
    pub timestamp_ns: u64,
    pub cpu_id: u8,
    pub class: u8,
    pub event_code: u8,
    pub payload: KevPayload,
}

pub struct KevTrace {
    pub header: KevHeader,
    pub events: Vec<KevEvent>,
}

impl KevTrace {
    pub fn new(header: KevHeader) -> Self {
        Self {
            header,
            events: Vec::new(),
        }
    }

    pub fn add_event(&mut self, event: KevEvent) {
        self.events.push(event);
        self.header.total_events = self.events.len() as u64;
    }

    /// Serialize trace into authentic QNX .kev binary stream
    pub fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.header.magic)?;
        w.write_u16::<LittleEndian>(self.header.major_version)?;
        w.write_u16::<LittleEndian>(self.header.minor_version)?;
        w.write_u16::<LittleEndian>(self.header.endian)?;
        w.write_u16::<LittleEndian>(self.header.cpu_arch)?;
        w.write_u32::<LittleEndian>(self.header.num_cpus)?;
        w.write_u64::<LittleEndian>(self.header.timestamp_freq_hz)?;
        w.write_u64::<LittleEndian>(self.events.len() as u64)?;

        for ev in &self.events {
            w.write_u64::<LittleEndian>(ev.timestamp_ns)?;
            w.write_u8(ev.cpu_id)?;
            w.write_u8(ev.class)?;
            w.write_u8(ev.event_code)?;
            w.write_u8(0)?; // padding byte

            match &ev.payload {
                KevPayload::CommMsg {
                    pid,
                    tid,
                    chid,
                    coid,
                    target_pid,
                    target_tid,
                    msg_size,
                    priority,
                } => {
                    let mut buf = Vec::new();
                    buf.write_u32::<LittleEndian>(*pid)?;
                    buf.write_u32::<LittleEndian>(*tid)?;
                    buf.write_u32::<LittleEndian>(*chid)?;
                    buf.write_u32::<LittleEndian>(*coid)?;
                    buf.write_u32::<LittleEndian>(*target_pid)?;
                    buf.write_u32::<LittleEndian>(*target_tid)?;
                    buf.write_u32::<LittleEndian>(*msg_size)?;
                    buf.write_u8(*priority)?;
                    buf.resize(32, 0); // fixed payload slot

                    w.write_u32::<LittleEndian>(buf.len() as u32)?;
                    w.write_all(&buf)?;
                }
                KevPayload::ThreadState {
                    pid,
                    tid,
                    priority,
                    inherited_priority,
                    channel_id,
                    blocked_on_pid,
                    blocked_on_tid,
                } => {
                    let mut buf = Vec::new();
                    buf.write_u32::<LittleEndian>(*pid)?;
                    buf.write_u32::<LittleEndian>(*tid)?;
                    buf.write_u8(*priority)?;
                    buf.write_u8(*inherited_priority)?;
                    buf.write_u16::<LittleEndian>(0)?; // pad
                    buf.write_u32::<LittleEndian>(*channel_id)?;
                    buf.write_u32::<LittleEndian>(*blocked_on_pid)?;
                    buf.write_u32::<LittleEndian>(*blocked_on_tid)?;
                    buf.resize(24, 0);

                    w.write_u32::<LittleEndian>(buf.len() as u32)?;
                    w.write_all(&buf)?;
                }
                KevPayload::ApsBudget {
                    partition_id,
                    budget_percent,
                    used_percent,
                    critical_budget_ms,
                    critical_used_ms,
                    num_overruns,
                } => {
                    let mut buf = Vec::new();
                    buf.write_u16::<LittleEndian>(*partition_id)?;
                    buf.write_u8(*budget_percent)?;
                    buf.write_u8(*used_percent)?;
                    buf.write_u32::<LittleEndian>(*critical_budget_ms)?;
                    buf.write_u32::<LittleEndian>(*critical_used_ms)?;
                    buf.write_u32::<LittleEndian>(*num_overruns)?;
                    buf.resize(20, 0);

                    w.write_u32::<LittleEndian>(buf.len() as u32)?;
                    w.write_all(&buf)?;
                }
                KevPayload::UserString {
                    pid,
                    tid,
                    event_code,
                    text,
                } => {
                    let bytes = text.as_bytes();
                    let total_len = 10 + bytes.len();
                    w.write_u32::<LittleEndian>(total_len as u32)?;
                    w.write_u32::<LittleEndian>(*pid)?;
                    w.write_u32::<LittleEndian>(*tid)?;
                    w.write_u16::<LittleEndian>(*event_code)?;
                    w.write_all(bytes)?;
                }
                KevPayload::RawBytes(bytes) => {
                    w.write_u32::<LittleEndian>(bytes.len() as u32)?;
                    w.write_all(bytes)?;
                }
            }
        }
        Ok(())
    }

    /// Read trace from binary reader
    pub fn read_from<R: Read>(r: &mut R) -> io::Result<Self> {
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if magic != KEV_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid QNX KEV magic: {:?}", String::from_utf8_lossy(&magic)),
            ));
        }

        let major_version = r.read_u16::<LittleEndian>()?;
        let minor_version = r.read_u16::<LittleEndian>()?;
        let endian = r.read_u16::<LittleEndian>()?;
        let cpu_arch = r.read_u16::<LittleEndian>()?;
        let num_cpus = r.read_u32::<LittleEndian>()?;
        let timestamp_freq_hz = r.read_u64::<LittleEndian>()?;
        let total_events = r.read_u64::<LittleEndian>()?;

        let header = KevHeader {
            magic,
            major_version,
            minor_version,
            endian,
            cpu_arch,
            num_cpus,
            timestamp_freq_hz,
            total_events,
        };

        let mut events = Vec::with_capacity(total_events as usize);

        for _ in 0..total_events {
            let timestamp_ns = r.read_u64::<LittleEndian>()?;
            let cpu_id = r.read_u8()?;
            let class = r.read_u8()?;
            let event_code = r.read_u8()?;
            let _pad = r.read_u8()?;
            let payload_len = r.read_u32::<LittleEndian>()? as usize;

            let mut payload_buf = vec![0u8; payload_len];
            r.read_exact(&mut payload_buf)?;
            let mut cur = Cursor::new(&payload_buf);

            let payload = match class {
                QNX_TRACE_CLASS_COMM => {
                    let pid = cur.read_u32::<LittleEndian>()?;
                    let tid = cur.read_u32::<LittleEndian>()?;
                    let chid = cur.read_u32::<LittleEndian>()?;
                    let coid = cur.read_u32::<LittleEndian>()?;
                    let target_pid = cur.read_u32::<LittleEndian>()?;
                    let target_tid = cur.read_u32::<LittleEndian>()?;
                    let msg_size = cur.read_u32::<LittleEndian>()?;
                    let priority = cur.read_u8()?;
                    KevPayload::CommMsg {
                        pid,
                        tid,
                        chid,
                        coid,
                        target_pid,
                        target_tid,
                        msg_size,
                        priority,
                    }
                }
                QNX_TRACE_CLASS_THREAD => {
                    let pid = cur.read_u32::<LittleEndian>()?;
                    let tid = cur.read_u32::<LittleEndian>()?;
                    let priority = cur.read_u8()?;
                    let inherited_priority = cur.read_u8()?;
                    let _pad16 = cur.read_u16::<LittleEndian>()?;
                    let channel_id = cur.read_u32::<LittleEndian>()?;
                    let blocked_on_pid = cur.read_u32::<LittleEndian>()?;
                    let blocked_on_tid = cur.read_u32::<LittleEndian>()?;
                    KevPayload::ThreadState {
                        pid,
                        tid,
                        priority,
                        inherited_priority,
                        channel_id,
                        blocked_on_pid,
                        blocked_on_tid,
                    }
                }
                QNX_TRACE_CLASS_APS => {
                    let partition_id = cur.read_u16::<LittleEndian>()?;
                    let budget_percent = cur.read_u8()?;
                    let used_percent = cur.read_u8()?;
                    let critical_budget_ms = cur.read_u32::<LittleEndian>()?;
                    let critical_used_ms = cur.read_u32::<LittleEndian>()?;
                    let num_overruns = cur.read_u32::<LittleEndian>()?;
                    KevPayload::ApsBudget {
                        partition_id,
                        budget_percent,
                        used_percent,
                        critical_budget_ms,
                        critical_used_ms,
                        num_overruns,
                    }
                }
                QNX_TRACE_CLASS_USER => {
                    if payload_buf.len() >= 10 {
                        let pid = cur.read_u32::<LittleEndian>()?;
                        let tid = cur.read_u32::<LittleEndian>()?;
                        let code = cur.read_u16::<LittleEndian>()?;
                        let text = String::from_utf8_lossy(&payload_buf[10..]).to_string();
                        KevPayload::UserString {
                            pid,
                            tid,
                            event_code: code,
                            text,
                        }
                    } else {
                        KevPayload::RawBytes(payload_buf)
                    }
                }
                _ => KevPayload::RawBytes(payload_buf),
            };

            events.push(KevEvent {
                timestamp_ns,
                cpu_id,
                class,
                event_code,
                payload,
            });
        }

        Ok(Self { header, events })
    }
}
