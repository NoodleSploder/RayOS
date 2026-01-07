use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(name = "rayos-installer")]
#[command(about = "RayOS installer dry-run planner", long_about = None)]
#[command(after_help = "By default this tool emits a SAMPLE disk layout. Pass --enumerate-local-disks to scan real hardware only inside installer/test VMs.")]
struct Cli {
    /// Output format for planner results
    #[arg(long, default_value_t = OutputFormat::Json)]
    output_format: OutputFormat,

    /// Emit extended debug information
    #[arg(long)]
    debug: bool,

    /// Enumerate block devices on the current machine (off by default)
    #[arg(long)]
    enumerate_local_disks: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum, Serialize)]
enum OutputFormat {
    Json,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum PlannerError {
    #[error("failed to enumerate block devices: {0}")]
    Enumerate(io::Error),
}

#[derive(Debug, Serialize)]
struct PlannerReport {
    timestamp_epoch_s: u64,
    disks: Vec<BlockDevice>,
}

#[derive(Debug, Serialize)]
struct BlockDevice {
    name: String,
    dev_path: String,
    is_removable: bool,
    is_read_only: Option<bool>,
    sector_size_bytes: u64,
    size_bytes: Option<u64>,
    partitions: Vec<Partition>,
}

#[derive(Debug, Serialize)]
struct Partition {
    name: String,
    dev_path: String,
    number: Option<u32>,
    size_bytes: Option<u64>,
}

fn main() -> anyhow::Result<()> {
    eprintln!("RAYOS_INSTALLER:STARTED");

    let cli = Cli::parse();
    let report = collect_install_plan(cli.enumerate_local_disks, cli.debug)?;

    eprintln!("RAYOS_INSTALLER:PLAN_GENERATED:disk_count={}", report.disks.len());

    match cli.output_format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&report)?;
            println!("{}", json);
            eprintln!("RAYOS_INSTALLER:JSON_EMITTED");
        }
    }

    eprintln!("RAYOS_INSTALLER:COMPLETE");
    Ok(())
}

fn collect_install_plan(enumerate: bool, debug: bool) -> Result<PlannerReport, PlannerError> {
    let timestamp_epoch_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if !enumerate {
        if debug {
            eprintln!(
                "INFO: returning sample installer layout (no local disk enumeration performed)"
            );
        }
        eprintln!("RAYOS_INSTALLER:SAMPLE_MODE");
        return Ok(PlannerReport {
            timestamp_epoch_s,
            disks: sample_disks(),
        });
    }

    eprintln!("RAYOS_INSTALLER:ENUMERATE_MODE");
    let mut disks = Vec::new();
    let sys_block = Path::new("/sys/block");
    if sys_block.is_dir() {
        let entries = fs::read_dir(sys_block).map_err(PlannerError::Enumerate)?;
        for entry in entries {
            let entry = entry.map_err(PlannerError::Enumerate)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_candidate_disk(&name) {
                continue;
            }
            let removable = read_removable(entry.path()).unwrap_or(false);
            let dev_path = format!("/dev/{}", name);
            let is_read_only = read_read_only(&entry.path());
            let size_bytes = read_size_bytes(&entry.path());
            let partitions = collect_partitions(&name, &entry.path(), debug);
            disks.push(BlockDevice {
                name: name.clone(),
                dev_path: dev_path.clone(),
                is_removable: removable,
                is_read_only,
                sector_size_bytes: SECTOR_SIZE_BYTES,
                size_bytes,
                partitions,
            });
            eprintln!("RAYOS_INSTALLER:DISK_DETECTED:name={}", name);
        }
    } else if debug {
        eprintln!("INFO: /sys/block not present; skipping disk enumeration");
    }

    Ok(PlannerReport {
        timestamp_epoch_s,
        disks,
    })
}

fn is_candidate_disk(name: &str) -> bool {
    // Filter obvious pseudo-devices; keep simple for the first pass.
    if name.starts_with("loop") || name.starts_with("ram") {
        return false;
    }
    // Skip device mapper snapshots for now (thin pools etc.), keep primary dm devices.
    if name.starts_with("dm-") {
        return false;
    }
    true
}

fn read_removable(path: impl AsRef<Path>) -> Option<bool> {
    let removable_path = path.as_ref().join("removable");
    let contents = fs::read_to_string(removable_path).ok()?;
    match contents.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn read_read_only(path: impl AsRef<Path>) -> Option<bool> {
    let ro_path = path.as_ref().join("ro");
    let contents = fs::read_to_string(ro_path).ok()?;
    match contents.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

const SECTOR_SIZE_BYTES: u64 = 512;

fn read_size_bytes(path: impl AsRef<Path>) -> Option<u64> {
    let size_path = path.as_ref().join("size");
    let contents = fs::read_to_string(size_path).ok()?;
    let sectors: u64 = contents.trim().parse().ok()?;
    Some(sectors.saturating_mul(SECTOR_SIZE_BYTES))
}

fn sample_disks() -> Vec<BlockDevice> {
    vec![BlockDevice {
        name: "sample0".to_string(),
        dev_path: "sample://disk0".to_string(),
        is_removable: false,
        is_read_only: Some(false),
        sector_size_bytes: SECTOR_SIZE_BYTES,
        size_bytes: Some(256 * 1024 * 1024 * 1024), // 256 GiB
        partitions: vec![
            Partition {
                name: "sample0p1".to_string(),
                dev_path: "sample://disk0p1".to_string(),
                number: Some(1),
                size_bytes: Some(512 * 1024 * 1024), // 512 MiB EFI system partition
            },
            Partition {
                name: "sample0p2".to_string(),
                dev_path: "sample://disk0p2".to_string(),
                number: Some(2),
                size_bytes: Some(40 * 1024 * 1024 * 1024), // 40 GiB RayOS system volume
            },
            Partition {
                name: "sample0p3".to_string(),
                dev_path: "sample://disk0p3".to_string(),
                number: Some(3),
                size_bytes: Some(200 * 1024 * 1024 * 1024), // 200 GiB VM storage pool
            },
        ],
    }]
}

fn collect_partitions(device: &str, device_sys_path: &Path, debug: bool) -> Vec<Partition> {
    let mut parts = Vec::new();
    let entries = match fs::read_dir(device_sys_path) {
        Ok(e) => e,
        Err(err) => {
            if debug {
                eprintln!(
                    "WARN: failed to read partitions for {}: {}",
                    device,
                    err
                );
            }
            return parts;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(device) {
            continue;
        }
        // Skip the device directory itself.
        if name == device {
            continue;
        }

        let sys_path = entry.path();
        if !sys_path.join("partition").exists() {
            continue;
        }

        let size_bytes = read_size_bytes(&sys_path);
        let number = parse_partition_number(device, &name);
        let dev_path = format!("/dev/{}", name);

        parts.push(Partition {
            name,
            dev_path,
            number,
            size_bytes,
        });
    }

    parts.sort_by_key(|p| p.number.unwrap_or(u32::MAX));
    parts
}

fn parse_partition_number(device: &str, child: &str) -> Option<u32> {
    if let Some(rest) = child.strip_prefix(device) {
        // NVMe devices end with p<number>, e.g., nvme0n1p1
        let digits = rest.trim_start_matches(|c: char| c == 'p');
        digits.parse().ok()
    } else {
        None
    }
}
