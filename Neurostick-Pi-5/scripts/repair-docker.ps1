# Elevated repair: clean uninstall + reinstall of Docker Desktop after a
# botched winget run leaves the install directory partly populated but
# missing critical files (frontend/bundle.js, resources/linuxkit/*.iso).

$ErrorActionPreference = "Continue"

function Step { param($Msg) Write-Host "==== $Msg ====" -ForegroundColor Cyan }

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "Need Administrator." -ForegroundColor Red
    Read-Host "Press Enter to close"
    exit 1
}

Step "1/6  Shut down WSL + Docker processes"
& wsl.exe --shutdown 2>&1 | Out-Null
Get-Process | Where-Object {
    $_.ProcessName -match 'Docker Desktop|com\.docker|dockerd|docker-compose|docker-credential|vpnkit|qemu-img'
} | ForEach-Object {
    Write-Host "  killing $($_.ProcessName) ($($_.Id))"
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}

Step "2/6  winget uninstall (best-effort)"
& winget.exe uninstall --id Docker.DockerDesktop --silent --accept-source-agreements 2>&1 | Out-Host
Write-Host "winget uninstall exit code: $LASTEXITCODE"

Step "3/6  Remove leftover directories"
$dirs = @(
    "$env:ProgramFiles\Docker",
    "$env:ProgramData\Docker",
    "$env:ProgramData\DockerDesktop",
    "$env:LOCALAPPDATA\Docker",
    "$env:APPDATA\Docker",
    "$env:APPDATA\Docker Desktop"
)
foreach ($d in $dirs) {
    if (Test-Path $d) {
        Write-Host "  removing $d"
        Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue
        if (Test-Path $d) {
            Write-Host "    WARNING: still present (in-use files?)" -ForegroundColor Yellow
        }
    }
}

Step "4/6  winget install fresh"
& winget.exe install --id Docker.DockerDesktop --source winget `
    --accept-source-agreements --accept-package-agreements --silent
$installRC = $LASTEXITCODE
Write-Host "winget install exit code: $installRC"

Step "5/6  Verify install integrity"
$root = "$env:ProgramFiles\Docker\Docker"
$mustHave = @(
    "Docker Desktop.exe",
    "frontend\bundle.js",
    "resources\linuxkit\docker-desktop.iso",
    "resources\install-scripts.ps1"
)
$ok = $true
foreach ($f in $mustHave) {
    $p = Join-Path $root $f
    if (Test-Path $p) {
        Write-Host "  ok  : $f"
    } else {
        Write-Host "  MISS: $f" -ForegroundColor Red
        $ok = $false
    }
}

Step "6/6  Result"
if ($ok) {
    Write-Host "Docker Desktop reinstall LOOKS HEALTHY." -ForegroundColor Green
    Write-Host "Next: launch 'Docker Desktop' from Start menu, accept license, wait for green whale."
} else {
    Write-Host "Reinstall is STILL incomplete." -ForegroundColor Red
    Write-Host "Try downloading installer manually:" -ForegroundColor Yellow
    Write-Host "  https://desktop.docker.com/win/main/amd64/Docker%20Desktop%20Installer.exe"
}

Write-Host ""
Write-Host "Window closes in 90 seconds, or press any key now."
$end = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $end) {
    if ([Console]::KeyAvailable) { [Console]::ReadKey($true) | Out-Null; break }
    Start-Sleep -Milliseconds 200
}
