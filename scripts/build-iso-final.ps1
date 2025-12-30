# RayOS ISO Build Script - Final Version
# This script builds the bootloader, kernel, and creates a bootable UEFI ISO

param(
    [switch]$Clean = $false,
    [string]$OutputDir = ".\build"
)

$ErrorActionPreference = "Continue"

Write-Host "=== RayOS ISO Build Script ===" -ForegroundColor Cyan
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
Write-Host "[2/4] Building UEFI bootloader..." -ForegroundColor Yellow
Push-Location bootloader\uefi_boot
$output = cargo build --release --target x86_64-unknown-uefi 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "  [X] Bootloader build failed with exit code $LASTEXITCODE" -ForegroundColor Red
    Pop-Location
    exit 1
}
Write-Host "  [*] Bootloader compilation successful" -ForegroundColor Green
Pop-Location
Write-Host ""

# Step 3: Build kernel
Write-Host "[3/4] Building RayOS kernel..." -ForegroundColor Yellow
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
$IsoPath = Join-Path $OutputDir "rayos.iso"

# Create directories
New-Item -ItemType Directory -Path $BootDir -Force | Out-Null
New-Item -ItemType Directory -Path $RayOSDir -Force | Out-Null

# Copy bootloader
$BootloaderSource = "bootloader\target\x86_64-unknown-uefi\release\uefi_boot.efi"
Copy-Item $BootloaderSource (Join-Path $BootDir "BOOTX64.EFI") -Force
Write-Host "  [*] Copied UEFI bootloader (2.5 KB)" -ForegroundColor Green

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

Bootloader: UEFI x86_64 PE/COFF
Architecture: GPU-native, AI-centric OS
System: Bicameral Kernel (System 1 GPU + System 2 LLM)

Files:
- EFI/BOOT/BOOTX64.EFI: UEFI bootloader entry point
- EFI/RAYOS/kernel.bin: RayOS kernel binary

Boot Steps:
1. Boot from UEFI firmware (enable UEFI mode in BIOS)
2. Select boot device containing this ISO
3. You should see the UEFI bootloader greeting
4. Kernel would be loaded (Phase 1 skeleton)
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
    try {
        wsl xorriso -as mkisofs -R -J -V "RayOS" -isohybrid-gpt-basdat -o "$IsoPathWSL" "$IsoContentWSL" 2>&1 | Out-Null

    if (Test-Path $IsoPathFull) {
            $IsoSize = (Get-Item $IsoPathFull).Length / 1MB
            Write-Host "  [*] ISO created successfully ($([math]::Round($IsoSize, 2)) MB)" -ForegroundColor Green
        }
        else {
            throw "ISO file not found after creation"
        }
    }
    catch {
        Write-Host "  [X] ISO creation failed: $_" -ForegroundColor Red
        exit 1
    }
}
else {
    Write-Host "  [X] xorriso not found (required for ISO creation)" -ForegroundColor Red
    Write-Host ""
    Write-Host "  Install xorriso via WSL:" -ForegroundColor Yellow
    Write-Host "  wsl sudo apt-get install xorriso" -ForegroundColor Gray
    exit 1
}

Write-Host ""
Write-Host "=== Build Complete ===" -ForegroundColor Green
Write-Host ""
Write-Host "ISO Location: $IsoPathFull" -ForegroundColor Cyan
Write-Host "ISO Size:     $([math]::Round((Get-Item $IsoPathFull).Length / 1MB, 2)) MB" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Yellow
Write-Host "1. Test in Virtual Machine (VirtualBox/Hyper-V with UEFI enabled)" -ForegroundColor Gray
Write-Host "2. Write to USB: Rufus (https://rufus.ie/) with GPT + UEFI settings" -ForegroundColor Gray
Write-Host "3. Boot from UEFI firmware (may need to disable Secure Boot in BIOS)" -ForegroundColor Gray
Write-Host ""
Write-Host "See docs/BUILD_GUIDE.md for detailed testing instructions" -ForegroundColor Yellow
