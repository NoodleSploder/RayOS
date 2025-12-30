# RayOS ISO Build Script for aarch64 (ARM64)
# For VM testing on ARM64 architecture

param(
    [switch]$Clean = $false,
    [string]$OutputDir = ".\build",
    [string]$Architecture = "aarch64"  # Can also be "x86_64"
)

$ErrorActionPreference = "Continue"

Write-Host "=== RayOS ISO Build Script (aarch64 ARM64) ===" -ForegroundColor Cyan
Write-Host ""

# Determine target triple based on architecture
if ($Architecture -eq "aarch64") {
    $BootloaderTarget = "aarch64-unknown-uefi"
    $KernelTarget = "aarch64-unknown-linux-gnu"
    $BootloaderBinary = "uefi_boot.efi"
    $BootX64EFI = "BOOTAA64.EFI"  # For aarch64
    Write-Host "Building for: aarch64 (ARM64 UEFI)" -ForegroundColor Yellow
}
else {
    $BootloaderTarget = "x86_64-unknown-uefi"
    $KernelTarget = "x86_64-pc-windows-msvc"
    $BootloaderBinary = "uefi_boot.efi"
    $BootX64EFI = "BOOTX64.EFI"  # For x86_64
    Write-Host "Building for: x86_64 (Intel/AMD)" -ForegroundColor Yellow
}

Write-Host ""

# Step 1: Clean if requested
if ($Clean) {
    Write-Host "[1/4] Cleaning previous builds..." -ForegroundColor Yellow
    if (Test-Path $OutputDir) {
        Remove-Item $OutputDir -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "  [*] Cleaned output directory" -ForegroundColor Green
    }
    Push-Location bootloader
    cargo clean 2>&1 | Out-Null
    Pop-Location
    Push-Location kernel
    cargo clean 2>&1 | Out-Null
    Pop-Location
    Write-Host ""
}

# Step 2: Build bootloader
Write-Host "[2/4] Building UEFI bootloader ($Architecture)..." -ForegroundColor Yellow
Push-Location bootloader\uefi_boot

if ($Architecture -eq "aarch64") {
    $output = cargo +nightly build -Zbuild-std=core --release --target $BootloaderTarget 2>&1
} else {
    $output = cargo build --release --target $BootloaderTarget 2>&1
}

if ($LASTEXITCODE -ne 0) {
    Write-Host "  [X] Bootloader build failed with exit code $LASTEXITCODE" -ForegroundColor Red
    Pop-Location
    exit 1
}
Write-Host "  [*] Bootloader compilation successful" -ForegroundColor Green
Pop-Location
Write-Host ""

# Step 3: Build kernel
Write-Host "[3/4] Building RayOS kernel ($Architecture)..." -ForegroundColor Yellow
Push-Location kernel
$output = cargo build --release 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "  [X] Kernel build failed with exit code $LASTEXITCODE" -ForegroundColor Red
    Pop-Location
    exit 1
}
Write-Host "  [*] Kernel compilation successful" -ForegroundColor Green
Pop-Location
Write-Host ""

# Step 4: Create ISO
Write-Host "[4/4] Creating bootable ISO..." -ForegroundColor Yellow

# Setup paths
$IsoContentDir = Join-Path $OutputDir "iso-content"
$BootDir = Join-Path $IsoContentDir "EFI\BOOT"
$RayOSDir = Join-Path $IsoContentDir "EFI\RAYOS"
$IsoPath = Join-Path $OutputDir "rayos-$Architecture.iso"

# Create directories
New-Item -ItemType Directory -Path $BootDir -Force | Out-Null
New-Item -ItemType Directory -Path $RayOSDir -Force | Out-Null

# Copy bootloader with correct name
if ($Architecture -eq "aarch64") {
    $BootloaderSource = "bootloader\target\aarch64-unknown-uefi\release\$BootloaderBinary"
} else {
    $BootloaderSource = "bootloader\target\x86_64-unknown-uefi\release\$BootloaderBinary"
}

Copy-Item $BootloaderSource (Join-Path $BootDir $BootX64EFI) -Force
Write-Host "  [*] Copied UEFI bootloader ($BootX64EFI)" -ForegroundColor Green

# Copy kernel
$KernelSource = "kernel\target\release\rayos-kernel.exe"
if (Test-Path $KernelSource) {
    Copy-Item $KernelSource (Join-Path $RayOSDir "kernel.bin") -Force
    $KernelSize = (Get-Item $KernelSource).Length / 1MB
    Write-Host "  [*] Copied kernel binary ($([math]::Round($KernelSize, 1)) MB)" -ForegroundColor Green
}
else {
    Write-Host "  [!] Kernel binary not found at $KernelSource" -ForegroundColor Yellow
}

# Create README
@"
RayOS Boot Information
======================

Architecture: ARM64 (aarch64) UEFI
Bootloader: UEFI aarch64 PE/COFF
System: Bicameral Kernel (System 1 GPU + System 2 LLM)

Files:
- EFI/BOOT/BOOTAA64.EFI: aarch64 UEFI bootloader entry point
- EFI/RAYOS/kernel.bin: RayOS kernel binary

Boot Steps:
1. Boot from UEFI firmware (aarch64 VM)
2. You should see the UEFI bootloader greeting
3. Kernel will be loaded (Phase 1 skeleton)
"@ | Out-File (Join-Path $IsoContentDir "README.txt") -Encoding UTF8
Write-Host "  [*] Created boot information file" -ForegroundColor Green

# Create ISO using xorriso via WSL
$RootDir = Get-Location
$IsoPathFull = Join-Path $RootDir $IsoPath
$IsoContentDirFull = Join-Path $RootDir $IsoContentDir

# Convert to WSL paths
$IsoPathWSL = $IsoPathFull -replace '\\', '/' -replace 'C:', '/mnt/c'
$IsoContentWSL = $IsoContentDirFull -replace '\\', '/' -replace 'C:', '/mnt/c'

if (Test-Path $IsoPathFull) {
    Remove-Item $IsoPathFull -Force
}

$xorrisoCheck = wsl which xorriso 2>&1 | Select-Object -First 1
if ($xorrisoCheck -and $xorrisoCheck -ne "") {
    wsl xorriso -as mkisofs -R -J -V "RayOS-$Architecture" -isohybrid-gpt-basdat -o "$IsoPathWSL" "$IsoContentWSL" 2>&1 | Out-Null

    if (Test-Path $IsoPathFull) {
        $IsoSize = (Get-Item $IsoPathFull).Length / 1MB
        Write-Host "  [*] ISO created successfully ($([math]::Round($IsoSize, 2)) MB)" -ForegroundColor Green
    }
    else {
        Write-Host "  [X] ISO creation failed" -ForegroundColor Red
        exit 1
    }
}
else {
    Write-Host "  [X] xorriso not found (required for ISO creation)" -ForegroundColor Red
    Write-Host "  Install: wsl sudo apt-get install xorriso" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "=== Build Complete ===" -ForegroundColor Green
Write-Host ""
Write-Host "ISO Location: $IsoPathFull" -ForegroundColor Cyan
Write-Host "Architecture: $Architecture (aarch64 ARM64)" -ForegroundColor Cyan
Write-Host "ISO Size:     $([math]::Round((Get-Item $IsoPathFull).Length / 1MB, 2)) MB" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Yellow
Write-Host "1. Mount ISO in your aarch64 VM" -ForegroundColor Gray
Write-Host "2. Boot from UEFI firmware" -ForegroundColor Gray
Write-Host "3. You should see: 'RayOS UEFI Bootloader v0.1'" -ForegroundColor Gray
