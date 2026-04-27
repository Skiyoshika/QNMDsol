# Elevated installer: WSL2 + Ubuntu + Docker Desktop, with retry path that
# heals the component store first when `Enable-WindowsOptionalFeature` reports
# error 14098 ("the component store has been corrupted").
#
# Triggered from `test-pi5-vm.bat install` or run directly from an admin shell.

$ErrorActionPreference = "Continue"

function Write-Step { param([string]$Msg) Write-Host "==== $Msg ====" -ForegroundColor Cyan }
function Pause-Then-Exit {
    param([int]$Code = 0, [int]$Seconds = 60)
    Write-Host ""
    Write-Host "Window closes in $Seconds seconds, or press any key now."
    $end = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $end) {
        if ([Console]::KeyAvailable) { [Console]::ReadKey($true) | Out-Null; break }
        Start-Sleep -Milliseconds 200
    }
    exit $Code
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "This script must run as Administrator. Aborting." -ForegroundColor Red
    Pause-Then-Exit -Code 1 -Seconds 10
}

function Try-EnableFeature {
    param([string]$Name)
    Write-Host ">> dism /online /enable-feature /featurename:$Name /all /norestart"
    & dism.exe /online /enable-feature /featurename:$Name /all /norestart
    return $LASTEXITCODE
}

Write-Step "Step 1/4  Enable VirtualMachinePlatform + WSL features"
$rc1 = Try-EnableFeature "VirtualMachinePlatform"
$rc2 = Try-EnableFeature "Microsoft-Windows-Subsystem-Linux"

# 14098 == ERROR_SXS_COMPONENT_STORE_CORRUPT. Either feature failing means we
# need to heal the component store before retrying.
$needsHeal = ($rc1 -eq 14098 -or $rc2 -eq 14098 -or $rc1 -eq -2146498548 -or $rc2 -eq -2146498548)
if ($needsHeal) {
    Write-Step "Step 1b  Component store reported as corrupt. Healing."
    Write-Host "This downloads replacement files from Windows Update; expect 5-30 min."
    & sfc.exe /scannow
    & dism.exe /online /cleanup-image /restorehealth
    Write-Host "Heal pass complete. Retrying feature enable..."
    $rc1 = Try-EnableFeature "VirtualMachinePlatform"
    $rc2 = Try-EnableFeature "Microsoft-Windows-Subsystem-Linux"
    if ($rc1 -ne 0 -and $rc1 -ne 3010) {
        Write-Host "VirtualMachinePlatform STILL failing (rc=$rc1). System needs deeper repair." -ForegroundColor Red
        Write-Host "Try Settings -> System -> Recovery -> Reset this PC -> 'Keep my files'." -ForegroundColor Red
        Pause-Then-Exit -Code 2
    }
}

Write-Step "Step 2/4  WSL kernel + Ubuntu (no auto-launch)"
& wsl.exe --update
& wsl.exe --set-default-version 2
& wsl.exe --install -d Ubuntu --no-launch
Write-Host "wsl --install exit code: $LASTEXITCODE"

Write-Step "Step 3/4  Docker Desktop via winget (idempotent)"
& winget.exe install --id Docker.DockerDesktop --source winget --accept-source-agreements --accept-package-agreements --silent
Write-Host "winget exit code: $LASTEXITCODE"

Write-Step "Step 4/4  Done"
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  1. REBOOT this machine."
Write-Host "  2. After reboot, launch 'Docker Desktop' once and accept the license."
Write-Host "  3. Wait until the Docker tray icon shows 'running' (whale solid)."
Write-Host "  4. Tell the assistant to continue."
Pause-Then-Exit -Code 0 -Seconds 90
