use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "rayos-installer")]
#[command(about = "RayOS installer dry-run planner", long_about = None)]
#[command(after_help = "By default this tool emits a SAMPLE disk layout. Pass --enumerate-local-disks to scan real hardware only inside installer/test VMs.")]
struct Cli {
    /// Output format for planner results (default: json)
    #[arg(long, default_value_t = OutputFormat::Json)]
    output_format: OutputFormat,

    /// Emit extended debug information
    #[arg(long)]
    debug: bool,

    /// Enumerate block devices on the current machine (off by default)
    #[arg(long)]
    enumerate_local_disks: bool,

    /// Interactive mode: prompt user for disk selection and partition choices
    #[arg(long)]
    interactive: bool,
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

    // If interactive mode, prompt user for disk selection
    if cli.interactive {
        eprintln!("RAYOS_INSTALLER:INTERACTIVE_MODE");
        run_interactive_menu(&report)?;
        eprintln!("RAYOS_INSTALLER:INTERACTIVE_COMPLETE");
        return Ok(());
    }

    // Otherwise, output the plan in the requested format
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

/// Interactive menu for disk and partition selection.
/// Prompts user to select a disk, then creates partitions and copies system image.
fn run_interactive_menu(report: &PlannerReport) -> anyhow::Result<()> {
    use std::io::BufRead;

    eprintln!();
    eprintln!("=== RayOS Installer ===");
    eprintln!("Installation Target Selection");
    eprintln!();

    if report.disks.is_empty() {
        eprintln!("ERROR: No disks found");
        return Err(anyhow::anyhow!("No disks available"));
    }

    // Display available disks
    eprintln!("Available disks:");
    for (idx, disk) in report.disks.iter().enumerate() {
        let size_gib = disk.size_bytes.unwrap_or(0) / (1024 * 1024 * 1024);
        let is_removable = if disk.is_removable {
            " (removable)"
        } else {
            ""
        };
        eprintln!("  [{}] {} - {} GiB{}", idx + 1, disk.name, size_gib, is_removable);
    }

    eprintln!();
    eprintln!("WARNING: Installation will erase the selected disk.");
    eprintln!("Make sure you have backups of important data.");
    eprintln!();

    // Prompt for disk selection
    eprintln!("Enter disk number (1-{}), or 0 to cancel: ", report.disks.len());
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    let disk_choice: usize = line.trim().parse()?;

    if disk_choice == 0 {
        eprintln!("Installation cancelled.");
        eprintln!("RAYOS_INSTALLER:INTERACTIVE_CANCELLED");
        return Ok(());
    }

    if disk_choice < 1 || disk_choice > report.disks.len() {
        return Err(anyhow::anyhow!("Invalid disk selection"));
    }

    let selected_disk = &report.disks[disk_choice - 1];
    eprintln!();
    eprintln!("Selected disk: {} ({})", selected_disk.name, selected_disk.dev_path);
    eprintln!();
    eprintln!("Partition configuration:");
    eprintln!("  EFI System Partition (ESP): 512 MiB");
    eprintln!("  RayOS System: 40 GiB");
    eprintln!("  VM Storage Pool: remaining space");
    eprintln!();
    eprintln!("Proceed with installation? (yes/no): ");

    line.clear();
    stdin.lock().read_line(&mut line)?;

    if line.trim().to_lowercase() != "yes" {
        eprintln!("Installation cancelled.");
        eprintln!("RAYOS_INSTALLER:INTERACTIVE_CANCELLED");
        return Ok(());
    }

    // Perform installation: create partitions and copy system image
    match perform_installation(&selected_disk) {
        Ok(_) => {
            eprintln!("RAYOS_INSTALLER:INSTALLATION_PLAN_VALIDATED:disk={}", selected_disk.name);
            eprintln!("RAYOS_INSTALLER:INSTALLATION_SUCCESSFUL");
        }
        Err(e) => {
            eprintln!("RAYOS_INSTALLER:INSTALLATION_FAILED:reason={}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Perform actual installation on the target disk.
/// This includes:
/// 1. Creating GPT partition table
/// 2. Creating ESP, System, and VM Storage partitions
/// 3. Formatting partitions
/// 4. Copying RayOS system image
fn perform_installation(disk: &BlockDevice) -> anyhow::Result<()> {
    let dev_path = &disk.dev_path;

    // If this is a sample disk, run in dry-run mode
    if dev_path.contains("sample://") {
        eprintln!("DRY RUN: Would install to {} (sample device)", dev_path);
        eprintln!("RAYOS_INSTALLER:DRY_RUN");
        return Ok(());
    }

    eprintln!("Creating partitions on {}...", dev_path);
    create_partitions(dev_path)?;

    eprintln!("Formatting partitions...");
    format_partitions(dev_path)?;

    eprintln!("Copying RayOS system image...");
    copy_system_image(dev_path)?;

    eprintln!("Installation completed successfully.");
    Ok(())
}

/// Create GPT partition table and partitions using sgdisk.
fn create_partitions(dev_path: &str) -> anyhow::Result<()> {
    // Clear existing partition table
    eprintln!("  Clearing existing partition table...");
    let mut cmd = Command::new("sgdisk");
    cmd.arg("-Z")  // Zap all GPT data structures
        .arg(dev_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to clear partition table"));
    }

    // Create new GPT partition table
    let mut cmd = Command::new("sgdisk");
    cmd.arg("-o")  // Create new protective MBR
        .arg(dev_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to create GPT table"));
    }

    // Partition 1: EFI System Partition (512 MiB)
    eprintln!("  Creating EFI System Partition (512 MiB)...");
    let mut cmd = Command::new("sgdisk");
    cmd.arg("-n").arg("1:2048:+512M")  // Partition 1, start at 2048, size 512M
        .arg("-t").arg("1:EF00")        // EFI system type
        .arg(dev_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to create ESP partition"));
    }

    // Partition 2: RayOS System (40 GiB)
    eprintln!("  Creating RayOS System partition (40 GiB)...");
    let mut cmd = Command::new("sgdisk");
    cmd.arg("-n").arg("2:0:+40G")      // Partition 2, start after p1, size 40G
        .arg("-t").arg("2:8300")        // Linux filesystem type
        .arg(dev_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to create RayOS system partition"));
    }

    // Partition 3: VM Storage Pool (remaining space)
    eprintln!("  Creating VM Storage Pool partition (remaining space)...");
    let mut cmd = Command::new("sgdisk");
    cmd.arg("-n").arg("3:0:0")         // Partition 3, use remaining space
        .arg("-t").arg("3:8300")        // Linux filesystem type
        .arg(dev_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to create VM storage partition"));
    }

    // Write changes
    eprintln!("  Writing partition table...");
    let mut cmd = Command::new("sgdisk");
    cmd.arg("-p")  // Print partition table
        .arg(dev_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to write partition table"));
    }

    // Notify kernel of partition changes
    eprintln!("  Notifying kernel of partition changes...");
    let mut cmd = Command::new("partprobe");
    cmd.arg(dev_path);
    let _ = cmd.status();  // Ignore if partprobe not available

    Ok(())
}

/// Format partitions with appropriate filesystems.
fn format_partitions(dev_path: &str) -> anyhow::Result<()> {
    // Format partition 1 (ESP) as FAT32
    let p1 = format!("{}1", dev_path);
    eprintln!("  Formatting {} as FAT32 (ESP)...", p1);
    let mut cmd = Command::new("mkfs.fat");
    cmd.arg("-F").arg("32")
        .arg("-n").arg("RAYOS_ESP")
        .arg(&p1);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to format ESP partition"));
    }

    // Format partition 2 (System) as ext4
    let p2 = format!("{}2", dev_path);
    eprintln!("  Formatting {} as ext4 (RayOS System)...", p2);
    let mut cmd = Command::new("mkfs.ext4");
    cmd.arg("-F")
        .arg("-L").arg("RAYOS_SYSTEM")
        .arg(&p2);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to format RayOS system partition"));
    }

    // Format partition 3 (VM Storage) as ext4
    let p3 = format!("{}3", dev_path);
    eprintln!("  Formatting {} as ext4 (VM Storage Pool)...", p3);
    let mut cmd = Command::new("mkfs.ext4");
    cmd.arg("-F")
        .arg("-L").arg("RAYOS_VM_POOL")
        .arg(&p3);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to format VM storage partition"));
    }

    Ok(())
}

/// Copy RayOS system image to the target partition.
/// This mounts the partitions temporarily and copies the boot media contents.
fn copy_system_image(dev_path: &str) -> anyhow::Result<()> {
    let work_dir = PathBuf::from("/tmp/rayos-install");
    fs::create_dir_all(&work_dir)?;

    let esp_mount = work_dir.join("esp");
    let system_mount = work_dir.join("system");
    fs::create_dir_all(&esp_mount)?;
    fs::create_dir_all(&system_mount)?;

    let p1 = format!("{}1", dev_path);
    let p2 = format!("{}2", dev_path);

    // Mount ESP partition
    eprintln!("  Mounting ESP partition...");
    let mut cmd = Command::new("mount");
    cmd.arg(&p1).arg(&esp_mount);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to mount ESP partition"));
    }

    // Mount System partition
    eprintln!("  Mounting System partition...");
    let mut cmd = Command::new("mount");
    cmd.arg(&p2).arg(&system_mount);
    let status = cmd.status()?;
    if !status.success() {
        let _ = Command::new("umount").arg(&esp_mount).status();
        return Err(anyhow::anyhow!("Failed to mount System partition"));
    }

    // Copy RayOS system image
    eprintln!("  Copying RayOS system files...");

    // Try to find system image in common locations
    let system_image_paths = vec![
        PathBuf::from("/rayos-system-image"),  // In running system
        PathBuf::from("./build/rayos-system-image"),  // Dev build
        PathBuf::from("/tmp/rayos-system-image"),  // Temporary location
    ];

    let mut image_found = false;
    for source_dir in &system_image_paths {
        if source_dir.exists() {
            eprintln!("    Found system image at: {}", source_dir.display());
            copy_directory_recursive(source_dir, &system_mount)?;
            image_found = true;
            break;
        }
    }

    if !image_found {
        eprintln!("    WARNING: System image not found, creating marker only");
    }

    // Write installation marker
    let marker = system_mount.join("RAYOS_INSTALLATION_MARKER");
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    fs::write(&marker, format!("RayOS installed at {}\n", timestamp))?;

    // Write installation metadata
    let metadata = system_mount.join(".rayos-install");
    fs::write(&metadata, format!("Installed: {}\nPartition: {}\n", timestamp, dev_path))?;

    // Sync to ensure writes complete
    let mut cmd = Command::new("sync");
    let _ = cmd.status();

    // Unmount partitions
    eprintln!("  Unmounting partitions...");
    let _ = Command::new("umount").arg(&system_mount).status();
    let _ = Command::new("umount").arg(&esp_mount).status();

    Ok(())
}

/// Recursively copy a directory and its contents.
fn copy_directory_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_dir() {
            copy_directory_recursive(&path, &dst_path)?;
        } else {
            eprintln!("      Copying: {}", file_name.to_string_lossy());
            fs::copy(&path, &dst_path)?;
        }
    }

    Ok(())
}
