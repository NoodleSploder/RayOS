# RayOS ISO Build Script
# This script builds the bootloader, kernel, and creates a bootable UEFI ISO

param(
    [switch]$Clean = $false,
    [string]$OutputDir = ".\iso-output"
)

$ErrorActionPreference = "Stop"
$RootDir = Get-Location
$IsoContentDir = Join-Path $OutputDir "iso-content"
$BootDir = Join-Path $IsoContentDir "EFI\BOOT"
$RayOSDir = Join-Path $IsoContentDir "EFI\RAYOS"

Write-Host "=== RayOS ISO Build Script ===" -ForegroundColor Cyan
Write-Host "Root Directory: $RootDir" -ForegroundColor Gray
Write-Host ""

# Step 1: Clean previous builds if requested
if ($Clean) {
    Write-Host "[1/5] Cleaning previous builds..." -ForegroundColor Yellow
    if (Test-Path $OutputDir) {
        Remove-Item $OutputDir -Recurse -Force
        Write-Host "  ✓ Cleaned output directory" -ForegroundColor Green
    }
    Push-Location bootloader
    cargo clean
    Pop-Location
    Push-Location kernel
    cargo clean
    Pop-Location
    Write-Host "  ✓ Cleaned cargo builds" -ForegroundColor Green
}

# Step 2: Build bootloader
Write-Host "[2/5] Building UEFI bootloader..." -ForegroundColor Yellow
Push-Location bootloader\uefi_boot
try {
    cargo build --release --target x86_64-unknown-uefi
    $BootloaderPath = "..\target\x86_64-unknown-uefi\release\uefi_boot.efi"
    if (-not (Test-Path $BootloaderPath)) {
        throw "Bootloader build failed or EFI not found at $BootloaderPath"
    }
    Write-Host "  ✓ Bootloader built successfully" -ForegroundColor Green
}
catch {
    Write-Host "  ✗ Bootloader build failed: $_" -ForegroundColor Red
    Pop-Location
    exit 1
}
Pop-Location

# Step 3: Build kernel
Write-Host "[3/5] Building RayOS kernel..." -ForegroundColor Yellow
Push-Location kernel
try {
    cargo build --release
    $KernelPath = ".\target\release\rayos-kernel.exe"
    # Try both .exe and non-.exe versions
    if (-not (Test-Path $KernelPath)) {
        $KernelPath = ".\target\release\rayos-kernel"
    }
    if (-not (Test-Path $KernelPath)) {
        Write-Host "  ⚠ Warning: Kernel executable not found, continuing anyway..." -ForegroundColor Yellow
    }
    else {
        Write-Host "  ✓ Kernel built successfully" -ForegroundColor Green
    }
}
catch {
    Write-Host "  ⚠ Warning: Kernel build had issues: $_" -ForegroundColor Yellow
}
Pop-Location

# Step 4: Create ISO structure
Write-Host "[4/5] Creating ISO directory structure..." -ForegroundColor Yellow
try {
    # Create directories
    New-Item -ItemType Directory -Path $BootDir -Force | Out-Null
    New-Item -ItemType Directory -Path $RayOSDir -Force | Out-Null
    Write-Host "  ✓ Created EFI directory structure" -ForegroundColor Green

    # Copy bootloader
    $BootloaderSource = Join-Path $RootDir "bootloader\target\x86_64-unknown-uefi\release\uefi_boot.efi"
    Copy-Item $BootloaderSource (Join-Path $BootDir "BOOTX64.EFI") -Force
    Write-Host "  ✓ Copied UEFI bootloader" -ForegroundColor Green

    # Copy kernel if it exists
    $KernelSource = Join-Path $RootDir "kernel\target\release\rayos-kernel"
    if (Test-Path $KernelSource) {
        Copy-Item $KernelSource (Join-Path $RayOSDir "kernel.bin") -Force
        Write-Host "  ✓ Copied kernel binary" -ForegroundColor Green
    } else {
        $KernelSource = Join-Path $RootDir "kernel\target\release\rayos-kernel.exe"
        if (Test-Path $KernelSource) {
            Copy-Item $KernelSource (Join-Path $RayOSDir "kernel.bin") -Force
            Write-Host "  ✓ Copied kernel binary" -ForegroundColor Green
        } else {
            Write-Host "  ⚠ Kernel binary not found, ISO will have bootloader only" -ForegroundColor Yellow
        }
    }

    # Create a boot.txt for reference
    @"
RayOS Boot Information
======================

Bootloader: UEFI x86_64
Architecture: GPU-native, AI-centric OS
System: Bicameral Kernel (System 1 + System 2)

Files:
- EFI/BOOT/BOOTX64.EFI: UEFI bootloader
- EFI/RAYOS/kernel.bin: RayOS kernel binary

Boot Method:
1. Insert USB or mount ISO
2. Boot from UEFI firmware (enable UEFI boot mode)
3. Select this device from boot menu
"@ | Out-File (Join-Path $IsoContentDir "README.txt")
    Write-Host "  ✓ Created boot information file" -ForegroundColor Green

}
catch {
    Write-Host "  ✗ ISO structure creation failed: $_" -ForegroundColor Red
    exit 1
}

# Step 5: Create ISO image
Write-Host "[5/5] Creating ISO image..." -ForegroundColor Yellow

$xorrisoFound = $false
try {
    $xorrisoPath = (Get-Command xorriso -ErrorAction Stop).Source
    $xorrisoFound = $true
}
catch {
    $wslCheck = wsl which xorriso 2>&1 | Select-Object -First 1
    if ($wslCheck -and $wslCheck -ne "") {
        $xorrisoFound = $true
    }
}

if ($xorrisoFound) {
    try {
        $IsoPath = Join-Path $OutputDir "rayos.iso"
        
        $useWSL = $false
        try {
            $xorrisoPath = (Get-Command xorriso -ErrorAction Stop).Source
        }
        catch {
            $useWSL = $true
        }
        
        if ($useWSL) {
            $wslIsoContent = ($IsoContentDir -replace '\\', '/').substring(0, 2) -eq 'C:' ? ("/mnt/" + $IsoContentDir[0].ToString().ToLower() + $IsoContentDir.Substring(2).Replace('\', '/')) : $IsoContentDir.Replace('\', '/')
            $wslIsoPath = ($IsoPath -replace '\\', '/').substring(0, 2) -eq 'C:' ? ("/mnt/" + $IsoPath[0].ToString().ToLower() + $IsoPath.Substring(2).Replace('\', '/')) : $IsoPath.Replace('\', '/')
            wsl xorriso -as mkisofs -R -J -V "RayOS" -isohybrid-gpt-basdat -o "$wslIsoPath" "$wslIsoContent"
        }
        else {
            & xorriso -as mkisofs -R -J -V "RayOS" -isohybrid-gpt-basdat -o $IsoPath $IsoContentDir
        }
        
        if (Test-Path $IsoPath) {
            $IsoSize = (Get-Item $IsoPath).Length / 1MB
            Write-Host "  ✓ ISO created successfully: $IsoPath ($([math]::Round($IsoSize, 2)) MB)" -ForegroundColor Green
        }
        else {
            throw "ISO file was not created"
        }
    }
    catch {
        Write-Host "  ✗ ISO creation failed: $_" -ForegroundColor Red
        exit 1
    }
}
else {
    Write-Host "  ✗ xorriso is required but not installed" -ForegroundColor Red
    Write-Host ""
    Write-Host "  Installation options:" -ForegroundColor Yellow
    Write-Host "    1. via WSL: wsl sudo apt-get install xorriso" -ForegroundColor Gray
    Write-Host "    2. Download from: https://www.gnu.org/software/xorriso/" -ForegroundColor Gray
    exit 1
}


Write-Host ""
Write-Host "=== Build Complete ===" -ForegroundColor Green
Write-Host ""
Write-Host "ISO Location: $(Join-Path $OutputDir 'rayos.iso')" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Yellow
Write-Host "1. Write to USB drive using one of these tools:" -ForegroundColor Gray
Write-Host "   - Rufus (Windows): https://rufus.ie/" -ForegroundColor Gray
Write-Host "   - Balena Etcher (Cross-platform): https://www.balena.io/etcher/" -ForegroundColor Gray
Write-Host "   - dd (Linux/Mac): dd if=rayos.iso of=/dev/sdX bs=4M" -ForegroundColor Gray
Write-Host ""
Write-Host "2. Or mount the ISO directly:" -ForegroundColor Gray
Write-Host "   - Double-click the ISO in File Explorer to mount" -ForegroundColor Gray
Write-Host "   - Or use: Mount-DiskImage -ImagePath '$(Join-Path $OutputDir 'rayos.iso')'" -ForegroundColor Gray
Write-Host ""
Write-Host "3. Boot from UEFI firmware (may need to enable UEFI boot in BIOS)" -ForegroundColor Gray
