# Measures a terminal the same way for every app, so baselines taken on different terminals compare.
[CmdletBinding(DefaultParameterSetName = 'Live')]
param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Live')]
    [string]$Name,
    [Parameter(ParameterSetName = 'Live')]
    [ValidateRange(1, 86400)]
    [int]$Seconds = 60,
    [Parameter(ParameterSetName = 'Live')]
    [ValidateRange(1, 3600)]
    [int]$Interval = 1,
    [Parameter(ParameterSetName = 'Live')]
    [string]$Csv,
    [Parameter(ParameterSetName = 'Live')]
    [int[]]$Exclude = @(),
    [Parameter(ParameterSetName = 'Live')]
    [switch]$Quiet,
    [Parameter(Mandatory = $true, ParameterSetName = 'Summary')]
    [string]$FromCsv
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$Cores = [Environment]::ProcessorCount
. (Join-Path $PSScriptRoot 'core.ps1')

if ($PSCmdlet.ParameterSetName -eq 'Summary') {
    $rows = @(Import-Csv -LiteralPath $FromCsv)
    if ($rows.Count -eq 0) { throw "No rows in $FromCsv" }
    # The file does not store the recording machine's core count, so name the basis without guessing it.
    Write-Summary $rows "the recording machine's logical cores"
    return
}

if ($Seconds -lt $Interval) { throw "-Seconds ($Seconds) must be at least -Interval ($Interval)" }
$skip = @($Exclude) + @($PID)
if ($Csv) {
    if (Test-Path -LiteralPath $Csv) { throw "$Csv exists; record each run to a new file so runs are never mixed" }
    $dir = Split-Path -Parent $Csv
    if ($dir -and -not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Path $dir) }
}

$all = New-Object System.Collections.ArrayList
$lastCpu = @{}
$clock = [Diagnostics.Stopwatch]::StartNew()
$lastTick = $null
$lastWall = $null
$samples = [math]::Floor($Seconds / $Interval)

if (-not $Quiet) {
    'Sampling {0} for {1} s every {2} s; CPU % of {3} logical cores' -f $Name, $Seconds, $Interval, $Cores
    '{0,-8} {1,4} | {2,6} {3,8} {4,8} | {5,6} {6,8} {7,8} | {8,6} {9,8} {10,8}' -f 'time', 'n', 'appCpu', 'appWs', 'appPriv', 'chCpu', 'chWs', 'chPriv', 'cpu', 'ws', 'priv'
}

# One extra snapshot first, because CPU needs a previous reading to diff against.
for ($i = 0; $i -le $samples; $i++) {
    $snapshot = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CreationDate, WorkingSetSize, PrivatePageCount, KernelModeTime, UserModeTime)
    $tick = $clock.Elapsed.Ticks
    $wall = Get-Date
    $tree = Get-TreeRow -Snapshot $snapshot -RootName $Name -Skip $skip
    if ($tree.Count -eq 0) { throw "No process named $Name is running" }

    $cpuNow = @{}
    $agg = @{}
    foreach ($t in $tree) {
        $p = $t.Process
        $key = '{0}|{1}' -f $p.ProcessId, $p.CreationDate.Ticks
        $cpu = [double]$p.KernelModeTime + [double]$p.UserModeTime
        $cpuNow[$key] = $cpu
        $delta = 0.0
        # Win32_Process cannot report a process after it exits, so CPU it used since the last snapshot is lost.
        if ($lastCpu.ContainsKey($key)) { $delta = $cpu - $lastCpu[$key] }
        elseif ($null -ne $lastWall -and $p.CreationDate -gt $lastWall) { $delta = $cpu }

        $aggKey = '{0}|{1}' -f $t.Group, $p.Name.ToLower()
        if (-not $agg.ContainsKey($aggKey)) {
            $agg[$aggKey] = [pscustomobject]@{ Group = $t.Group; Name = $p.Name.ToLower(); Count = 0; Cpu100ns = 0.0; Ws = 0.0; Priv = 0.0 }
        }
        $a = $agg[$aggKey]
        $a.Count++
        $a.Cpu100ns += $delta
        $a.Ws += [double]$p.WorkingSetSize
        # PrivatePageCount is in bytes despite its name.
        $a.Priv += [double]$p.PrivatePageCount
    }

    if ($null -ne $lastTick) {
        $span = [double]($tick - $lastTick)
        $stamp = $wall.ToString('HH:mm:ss')
        $sampleRows = foreach ($a in $agg.Values) {
            [pscustomobject]@{
                Time = (Get-SampleStamp $wall); Group = $a.Group; Name = $a.Name; Count = $a.Count
                Cpu = [math]::Round($a.Cpu100ns / ($span * $Cores) * 100, 2)
                WsMb = [math]::Round($a.Ws / 1MB, 1); PrivMb = [math]::Round($a.Priv / 1MB, 1)
            }
        }
        $sampleRows = @($sampleRows)
        foreach ($r in $sampleRows) { [void]$all.Add($r) }
        # The header goes in with the first rows, so a run that fails before sampling leaves no file to block a rerun.
        if ($Csv) { $sampleRows | ConvertTo-Csv -NoTypeInformation | Select-Object -Skip ([int](Test-Path -LiteralPath $Csv)) | Add-Content -LiteralPath $Csv -Encoding UTF8 }

        if (-not $Quiet) { Format-SampleLine -Rows $sampleRows -Clock $stamp -ProcessCount $tree.Count }
    }

    $lastCpu = $cpuNow
    $lastTick = $tick
    $lastWall = $wall
    if ($i -lt $samples) {
        # Sleep to a fixed schedule so query time does not stretch the run.
        $wait = ($i + 1) * $Interval * 1000 - $clock.Elapsed.TotalMilliseconds
        if ($wait -gt 0) { Start-Sleep -Milliseconds ([int]$wait) }
    }
}

if (-not $Quiet) { Write-Summary $all.ToArray() ('{0} logical cores' -f $Cores) }
