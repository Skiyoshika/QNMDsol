param(
    [switch]$InstallWSL,
    [switch]$InstallDocker
)

$ErrorActionPreference = "Stop"

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Command-Exists {
    param([string]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

$isAdmin = Test-Admin
$result = [ordered]@{
    admin = $isAdmin
    winget = (Command-Exists "winget")
    wsl_command = (Command-Exists "wsl")
    docker_command = (Command-Exists "docker")
    wsl_status_ok = $false
    wsl_distros_ok = $false
    actions = @()
}

if ($InstallWSL -or $InstallDocker) {
    if (-not $isAdmin) {
        throw "Installation requires an elevated PowerShell. Re-run as Administrator."
    }
}

if ($InstallWSL) {
    Write-Host "Installing WSL platform. A reboot may be required."
    wsl --install --no-distribution
    $result.actions += "wsl-install-requested"
}

if ($InstallDocker) {
    if (-not (Command-Exists "winget")) {
        throw "winget is required for unattended Docker Desktop installation."
    }
    Write-Host "Installing Docker Desktop via winget."
    winget install --id Docker.DockerDesktop --source winget --accept-source-agreements --accept-package-agreements
    $result.actions += "docker-desktop-install-requested"
}

if (Command-Exists "wsl") {
    & wsl --status *> $null
    $result.wsl_status_ok = ($LASTEXITCODE -eq 0)
    & wsl -l -v *> $null
    $result.wsl_distros_ok = ($LASTEXITCODE -eq 0)
    if (-not $result.wsl_status_ok -or -not $result.wsl_distros_ok) {
        $result.wsl_hint = "WSL is not initialized. Run this script as Administrator with -InstallWSL, reboot if requested, then install a distro."
    }
}

if (Command-Exists "docker") {
    try {
        $result.docker_version = (docker version --format "{{json .}}" 2>&1) -join "`n"
        $result.docker_buildx = (docker buildx version 2>&1) -join "`n"
    } catch {
        $result.docker_error = $_.Exception.Message
    }
}

$result | ConvertTo-Json -Depth 8
