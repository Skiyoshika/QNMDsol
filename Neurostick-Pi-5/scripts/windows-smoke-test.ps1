param(
    [string]$SerialPort = "",
    [int]$BoardId = 2,
    [int]$HttpPort = 18765,
    [int]$RecordMs = 3200,
    [switch]$SkipHardware,
    [switch]$SkipSlow,
    [switch]$NoBuild,
    [string]$DataRoot = ""
)

$ErrorActionPreference = "Stop"

function Repo-Root {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Invoke-Checked {
    param([string]$FilePath, [string[]]$Arguments)
    Write-Host "==> $FilePath $($Arguments -join ' ')"
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath exited with code $LASTEXITCODE"
    }
}

function Wait-Health {
    param([string]$BaseUrl, [int]$TimeoutSec = 30)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            Invoke-RestMethod -Uri "$BaseUrl/health" -TimeoutSec 1 | Out-Null
            return
        } catch {
            Start-Sleep -Milliseconds 250
        }
    }
    throw "Service did not become healthy at $BaseUrl"
}

function Stop-EdgeProcess {
    param($Process)
    if ($Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force
    }
}

function Start-Edge {
    param([string[]]$Arguments, [string]$RunDir)
    New-Item -ItemType Directory -Force $RunDir | Out-Null
    $exe = Join-Path (Repo-Root) "target\debug\pi_edge.exe"
    $stdout = Join-Path $RunDir "stdout.log"
    $stderr = Join-Path $RunDir "stderr.log"
    return Start-Process -FilePath $exe -ArgumentList $Arguments -WorkingDirectory (Repo-Root) `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr -WindowStyle Hidden -PassThru
}

function Invoke-SimSmoke {
    param([string]$RunDir)
    $port = $HttpPort
    $base = "http://127.0.0.1:$port"
    $dataDir = Join-Path $RunDir "data"
    $proc = Start-Edge -RunDir $RunDir -Arguments @(
        "--host", "127.0.0.1",
        "--port", "$port",
        "--simulate",
        "--data-dir", $dataDir
    )
    try {
        Wait-Health -BaseUrl $base
        $connect = Invoke-RestMethod -Method Post -Uri "$base/connect" -TimeoutSec 10
        $start = Invoke-RestMethod -Method Post -Uri "$base/start" -TimeoutSec 10
        Start-Sleep -Seconds 3
        $status = Invoke-RestMethod -Uri "$base/status" -TimeoutSec 10
        $decision = Invoke-RestMethod -Uri "$base/decision" -TimeoutSec 10
        $snapshot = Invoke-RestMethod -Uri "$base/snapshot" -TimeoutSec 10
        $stop = Invoke-RestMethod -Method Post -Uri "$base/stop" -TimeoutSec 10

        if (-not $connect.ok -or -not $start.ok -or -not $status.streaming -or -not $status.simulating) {
            throw "Simulation service did not enter expected streaming state"
        }
        if ($snapshot.channels.Count -lt 1 -or $snapshot.channels[0].Count -lt 1) {
            throw "Simulation snapshot is empty"
        }

        return [pscustomobject]@{
            ok = $true
            sample_rate_hz = $status.sample_rate_hz
            eeg_channels = $status.eeg_channels
            decision = $decision.decision
            snapshot_channels = $snapshot.channels.Count
            snapshot_first_channel_samples = $snapshot.channels[0].Count
            stopped = $stop.ok
        }
    } finally {
        Stop-EdgeProcess $proc
    }
}

function Invoke-HardwareSmoke {
    param([string]$RunDir, [string]$PortName)
    $port = $HttpPort + 1
    $base = "http://127.0.0.1:$port"
    $dataDir = Join-Path $RunDir "data"
    $proc = Start-Edge -RunDir $RunDir -Arguments @(
        "--host", "127.0.0.1",
        "--port", "$port",
        "--serial-port", $PortName,
        "--board-id", "$BoardId",
        "--data-dir", $dataDir
    )
    try {
        Wait-Health -BaseUrl $base -TimeoutSec 45
        $connect = Invoke-RestMethod -Method Post -Uri "$base/connect" -TimeoutSec 90
        $start = Invoke-RestMethod -Method Post -Uri "$base/start" -TimeoutSec 30
        Start-Sleep -Seconds 1
        $status = Invoke-RestMethod -Uri "$base/status" -TimeoutSec 10
        $recordStart = Invoke-RestMethod -Method Post -Uri "$base/record/start" -TimeoutSec 10
        $recordStartedAt = Get-Date
        Start-Sleep -Milliseconds $RecordMs
        $recordStop = Invoke-RestMethod -Method Post -Uri "$base/record/stop" -TimeoutSec 30
        $recordStoppedAt = Get-Date
        $snapshot = Invoke-RestMethod -Uri "$base/snapshot" -TimeoutSec 10
        $decision = Invoke-RestMethod -Uri "$base/decision" -TimeoutSec 10
        $stop = Invoke-RestMethod -Method Post -Uri "$base/stop" -TimeoutSec 30

        $samplesFile = Get-ChildItem -Path $dataDir -Recurse -Filter samples.csv | Select-Object -First 1
        if (-not $samplesFile) {
            throw "No samples.csv was written"
        }

        $lines = (Get-Content -Path $samplesFile.FullName | Measure-Object -Line).Lines
        $sampleRows = [Math]::Max(0, $lines - 1)
        $elapsedSec = ($recordStoppedAt - $recordStartedAt).TotalSeconds
        $expected = [Math]::Max(1, [Math]::Round($elapsedSec * [double]$status.sample_rate_hz))
        $minRows = [Math]::Floor($expected * 0.65)
        $maxRows = [Math]::Ceiling($expected * 1.35)
        if ($sampleRows -lt $minRows -or $sampleRows -gt $maxRows) {
            throw "Hardware sample rows out of rate bounds: rows=$sampleRows expected=$expected bounds=$minRows..$maxRows"
        }

        $quality = Measure-SampleQuality -Path $samplesFile.FullName

        return [pscustomobject]@{
            ok = $true
            serial_port = $PortName
            board_id = $BoardId
            sample_rate_hz = $status.sample_rate_hz
            eeg_channels = $status.eeg_channels
            record_elapsed_sec = [Math]::Round($elapsedSec, 3)
            sample_rows = $sampleRows
            expected_rows = $expected
            samples_csv = $samplesFile.FullName
            session_dir = $recordStop.session_dir
            decision = $decision.decision
            snapshot_channels = $snapshot.channels.Count
            snapshot_first_channel_samples = $snapshot.channels[0].Count
            saturated_ratio = $quality.saturated_ratio
            ranges_first_4 = $quality.ranges_first_4
            stopped = $stop.ok
        }
    } finally {
        Stop-EdgeProcess $proc
    }
}

function Measure-SampleQuality {
    param([string]$Path)
    $rows = Import-Csv -Path $Path
    if (-not $rows -or $rows.Count -eq 0) {
        return [pscustomobject]@{ saturated_ratio = $null; ranges_first_4 = @{} }
    }
    $channels = $rows[0].PSObject.Properties.Name | Where-Object { $_ -ne "t_sec" }
    $total = 0
    $saturated = 0
    $ranges = [ordered]@{}
    foreach ($ch in $channels) {
        $values = foreach ($row in $rows) { [double]$row.$ch }
        $total += $values.Count
        $saturated += @($values | Where-Object { [Math]::Abs($_) -ge 187000 }).Count
        if ($ranges.Count -lt 4) {
            $ranges[$ch] = @(
                [Math]::Round(($values | Measure-Object -Minimum).Minimum, 6),
                [Math]::Round(($values | Measure-Object -Maximum).Maximum, 6)
            )
        }
    }
    return [pscustomobject]@{
        saturated_ratio = if ($total -gt 0) { [Math]::Round($saturated / $total, 4) } else { $null }
        ranges_first_4 = $ranges
    }
}

$root = Repo-Root
Set-Location $root
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
if ([string]::IsNullOrWhiteSpace($DataRoot)) {
    $DataRoot = Join-Path $root "target\test-env\$timestamp"
}
New-Item -ItemType Directory -Force $DataRoot | Out-Null

$report = [ordered]@{
    created_at = (Get-Date).ToString("o")
    root = $root
    data_root = $DataRoot
    checks = [ordered]@{}
}

if (-not $NoBuild) {
    if (-not $SkipSlow) {
        Invoke-Checked "cargo" @("test", "--quiet")
        Invoke-Checked "cargo" @("test", "--test", "ssvep_decoder", "--quiet")
        Invoke-Checked "cargo" @("test", "--test", "edge_recording", "--quiet")
        Invoke-Checked "python" @("-m", "unittest", "trainer\test_data_contract.py")
    }
    Invoke-Checked "cargo" @("build", "--bin", "pi_edge", "--quiet")
}

$report.checks.simulation = Invoke-SimSmoke -RunDir (Join-Path $DataRoot "simulation")

if (-not $SkipHardware) {
    if ([string]::IsNullOrWhiteSpace($SerialPort)) {
        $ports = [System.IO.Ports.SerialPort]::GetPortNames()
        if ($ports.Count -eq 0) {
            throw "No serial ports found. Pass -SkipHardware or connect OpenBCI."
        }
        $SerialPort = $ports[0]
    }
    $report.checks.hardware = Invoke-HardwareSmoke -RunDir (Join-Path $DataRoot "hardware") -PortName $SerialPort
} else {
    $report.checks.hardware = [pscustomobject]@{ skipped = $true }
}

$reportPath = Join-Path $DataRoot "windows-smoke-report.json"
$report | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 $reportPath
Write-Host "Report: $reportPath"
$report | ConvertTo-Json -Depth 12
