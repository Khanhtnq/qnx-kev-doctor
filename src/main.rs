//! QNX KEV Doctor CLI Driver
//! Deterministic Debugging of QNX Neutrino 8.0 Microkernel IPC, Priority Inversion & APS

use clap::{Parser, Subcommand};
use colored::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use qnx_kev_doctor::html_report::generate_html_report;
use qnx_kev_doctor::sarif::generate_sarif_report;
use qnx_kev_doctor::{audit_kev_file, audit_trace, generate_synthetic_qnx8_trace, AuditReport};

#[derive(Parser)]
#[command(name = "qnx-kev-doctor")]
#[command(author = "Khanhtnq <218711032+Khanhtnq@users.noreply.github.com>")]
#[command(version = "1.0.0")]
#[command(about = "QNX Neutrino 8.0 Kernel Event Trace (.kev) Diagnostic Doctor", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run an end-to-end benchmark demo with synthetic QNX 8.0 kernel trace
    Demo {
        /// Path to export interactive HTML dashboard
        #[arg(long, default_value = "docs/index.html")]
        out_html: PathBuf,

        /// Path to export ISO 26262 SARIF report
        #[arg(long, default_value = "reports/qnx_audit.sarif")]
        out_sarif: PathBuf,

        /// Path to save the generated binary .kev trace
        #[arg(long, default_value = "reports/demo_synthetic.kev")]
        out_kev: PathBuf,
    },

    /// Analyze a real QNX tracelogger binary .kev file
    Analyze {
        /// Input .kev trace path
        #[arg(short, long)]
        input: PathBuf,

        /// Optional HTML report output
        #[arg(long)]
        out_html: Option<PathBuf>,

        /// Optional SARIF report output
        #[arg(long)]
        out_sarif: Option<PathBuf>,
    },

    /// Generate an authentic synthetic QNX 8.0 .kev trace for offline testing
    Generate {
        #[arg(short, long, default_value = "reports/sample_qnx8.kev")]
        output: PathBuf,
    },
}

fn print_report_summary(report: &AuditReport) {
    println!("{}", "================================================================================".cyan());
    println!("{}", "       QNX NEUTRINO 8.0 MICROKERNEL TRACE DOCTOR (AUDIT SUMMARY)               ".bold().cyan());
    println!("{}", "================================================================================".cyan());
    println!("Total Kernel Events Analyzed : {}", report.total_events_analyzed.to_string().bold());
    println!();

    // 1. Deadlock Section
    if report.deadlocks.is_empty() {
        println!("{} No circular IPC wait cycles detected.", "[PASS]".green().bold());
    } else {
        println!("{} {} CIRCULAR IPC DEADLOCK CYCLES DETECTED!", "[FAIL]".red().bold(), report.deadlocks.len());
        for dl in &report.deadlocks {
            println!(
                "  {} Cycle #{}: Participants: {} | Channels: {:?}",
                "->".red(),
                dl.cycle_id,
                dl.participants
                    .iter()
                    .map(|p| format!("PID {} (TID {})", p.pid, p.tid))
                    .collect::<Vec<_>>()
                    .join(" -> "),
                dl.channels_involved
            );
            println!("     Detail: {}", dl.explanation.yellow());
        }
    }
    println!();

    // 2. Priority Inversion Section
    if report.priority_inversions.is_empty() {
        println!("{} No priority inheritance violations detected.", "[PASS]".green().bold());
    } else {
        println!("{} {} PRIORITY INVERSIONS DETECTED!", "[WARN]".yellow().bold(), report.priority_inversions.len());
        for inv in &report.priority_inversions {
            println!(
                "  {} Violation #{}: Client PID {} (Prio {}) -> Server PID {} (Prio {}) on Chid {}",
                "->".yellow(),
                inv.violation_id,
                inv.client_pid,
                inv.client_priority.to_string().red(),
                inv.server_pid,
                inv.server_priority.to_string().yellow(),
                inv.channel_id
            );
            println!("     Root Cause: {}", inv.root_cause);
        }
    }
    println!();

    // 3. WCET Latency Profile
    println!("{}", "[PERF] Channel Roundtrip Latency (Nanoseconds WCET):".bold());
    println!(
        "  {:<12} {:<10} {:<10} {:<10} {:<10} {:<12} {:<10}",
        "Channel", "Samples", "Min(ns)", "p50(ns)", "p90(ns)", "p99.9(ns)", "Max(ns)"
    );
    for s in &report.latency_stats {
        println!(
            "  Channel {:<4} {:<10} {:<10} {:<10} {:<10} {:<12} {:<10}",
            s.channel_id, s.sample_count, s.min_ns, s.p50_ns, s.p90_ns, s.p999_ns.to_string().cyan(), s.max_ns
        );
    }
    println!();

    // 4. APS Partitions
    println!("{}", "[APS] Adaptive Partitioning Scheduler (Health Audit):".bold());
    for p in &report.aps_partitions {
        let status = if p.is_starved {
            "STARVED".red()
        } else if p.total_overruns > 0 {
            "OVERRUN_DETECTED".yellow()
        } else {
            "HEALTHY".green()
        };
        println!(
            "  Part #{:<2} ({:<22}) | Budget: {:>2}% | MaxUsed: {:>2}% | Overruns: {:>2} | [{}]",
            p.partition_id, p.name, p.configured_budget_percent, p.max_used_percent, p.total_overruns, status
        );
    }
    println!("{}", "================================================================================".cyan());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Demo {
            out_html,
            out_sarif,
            out_kev,
        } => {
            println!("{}", "Generating authentic synthetic QNX Neutrino 8.0 .kev trace...".cyan());
            let trace = generate_synthetic_qnx8_trace();

            // Save .kev
            if let Some(parent) = out_kev.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = File::create(&out_kev)?;
            let mut writer = BufWriter::new(file);
            trace.write_to(&mut writer)?;
            println!("Saved trace binary: {}", out_kev.display().to_string().green());

            // Run Audit
            let report = audit_trace(&trace);
            print_report_summary(&report);

            // Export SARIF 2.1.0
            generate_sarif_report(
                &report.deadlocks,
                &report.priority_inversions,
                &report.aps_anomalies,
                &out_sarif,
            )?;
            println!("Exported SARIF 2.1.0: {}", out_sarif.display().to_string().green());

            // Export HTML Report
            generate_html_report(
                &report.deadlocks,
                &report.priority_inversions,
                &report.latency_stats,
                &report.aps_partitions,
                &report.aps_anomalies,
                &out_html,
            )?;
            println!("Exported Interactive HTML: {}", out_html.display().to_string().green());
        }

        Commands::Analyze {
            input,
            out_html,
            out_sarif,
        } => {
            println!("Auditing QNX .kev file: {}", input.display());
            let report = audit_kev_file(&input)?;
            print_report_summary(&report);

            if let Some(sarif_path) = out_sarif {
                generate_sarif_report(
                    &report.deadlocks,
                    &report.priority_inversions,
                    &report.aps_anomalies,
                    &sarif_path,
                )?;
                println!("Exported SARIF 2.1.0: {}", sarif_path.display().to_string().green());
            }

            if let Some(html_path) = out_html {
                generate_html_report(
                    &report.deadlocks,
                    &report.priority_inversions,
                    &report.latency_stats,
                    &report.aps_partitions,
                    &report.aps_anomalies,
                    &html_path,
                )?;
                println!("Exported Interactive HTML: {}", html_path.display().to_string().green());
            }
        }

        Commands::Generate { output } => {
            let trace = generate_synthetic_qnx8_trace();
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = File::create(&output)?;
            let mut writer = BufWriter::new(file);
            trace.write_to(&mut writer)?;
            println!("Generated {} events into {}", trace.events.len(), output.display().to_string().green());
        }
    }

    Ok(())
}
