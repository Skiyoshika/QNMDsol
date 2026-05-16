param(
    [string]$Image = "neurostick-pi5-edge:local",
    [int]$HttpPort = 18780,
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"

# docker CLI prints its progress UI on stderr (`#0 building with ...`). Under
# `$ErrorActionPreference = Stop` PowerShell otherwise treats every stderr
# line as a NativeCommandError and aborts the script. Redirect explicitly.
function Invoke-Docker {
    param([Parameter(ValueFromRemainingArguments)] [string[]]$Args)
    & docker @Args 2>&1 | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) {
        throw "docker $Args failed with exit code $LASTEXITCODE"
    }
}

function Repo-Root {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Wait-Health {
    param([string]$BaseUrl, [int]$TimeoutSec = 60)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            Invoke-RestMethod -Uri "$BaseUrl/health" -TimeoutSec 2 | Out-Null
            return
        } catch {
            Start-Sleep -Seconds 1
        }
    }
    throw "Container did not become healthy at $BaseUrl"
}

Set-Location (Repo-Root)

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw "docker is not installed or not on PATH."
}

Invoke-Docker buildx version

if (-not $NoBuild) {
    Invoke-Docker buildx build --platform linux/arm64 `
        -f Neurostick-Pi-5/Dockerfile -t $Image --load `
        --progress plain .
}

$inspectJson = & docker image inspect $Image 2>&1 | Out-String
$inspect = $inspectJson | ConvertFrom-Json
if ($inspect[0].Architecture -ne "arm64") {
    throw "Image architecture is $($inspect[0].Architecture), expected arm64."
}

$containerName = "neurostick-pi5-arm64-smoke"
& docker rm -f $containerName 2>&1 | Out-Null
Invoke-Docker run -d --rm --name $containerName --platform linux/arm64 `
    -p "$HttpPort`:8765" `
    -e NEUROSTICK_SIMULATE=true `
    -e NEUROSTICK_DATA_DIR=/data `
    $Image

try {
    $base = "http://127.0.0.1:$HttpPort"
    Wait-Health -BaseUrl $base
    $connect = Invoke-RestMethod -Method Post -Uri "$base/connect" -TimeoutSec 10
    $start = Invoke-RestMethod -Method Post -Uri "$base/start" -TimeoutSec 10
    Start-Sleep -Seconds 3
    $status = Invoke-RestMethod -Uri "$base/status" -TimeoutSec 10
    $decision = Invoke-RestMethod -Uri "$base/decision" -TimeoutSec 10
    $snapshot = Invoke-RestMethod -Uri "$base/snapshot" -TimeoutSec 10
    $stop = Invoke-RestMethod -Method Post -Uri "$base/stop" -TimeoutSec 10

    [pscustomobject]@{
        image = $Image
        architecture = $inspect[0].Architecture
        os = $inspect[0].Os
        connect_ok = $connect.ok
        start_ok = $start.ok
        streaming = $status.streaming
        simulating = $status.simulating
        sample_rate_hz = $status.sample_rate_hz
        eeg_channels = $status.eeg_channels
        decision = $decision.decision
        snapshot_channels = $snapshot.channels.Count
        snapshot_first_channel_samples = $snapshot.channels[0].Count
        stop_ok = $stop.ok
    } | ConvertTo-Json -Depth 8
} finally {
    & docker rm -f $containerName 2>&1 | Out-Null
}
