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
. (Join-Path $PSScriptRoot 'tree.ps1')

function Get-Percentile {
    param([double[]]$Sorted, [double]$P)
    $rank = [math]::Ceiling($P / 100 * $Sorted.Count)
    return $Sorted[[math]::Max(0, $rank - 1)]
}

function Write-Summary {
    param([object[]]$Rows, [string]$Basis)

    $byTime = [ordered]@{}
    foreach ($r in $Rows) {
        if (-not $byTime.Contains($r.Time)) { $byTime[$r.Time] = New-Object System.Collections.ArrayList }
        [void]$byTime[$r.Time].Add($r)
    }
    $groups = @('app', 'child', 'total')
    $metrics = @('Cpu', 'WsMb', 'PrivMb')
    $perSample = @{}
    foreach ($g in $groups) { foreach ($m in $metrics) { $perSample["$g.$m"] = New-Object System.Collections.ArrayList } }

    foreach ($at in $byTime.Values) {
        foreach ($g in $groups) {
            $inGroup = $at
            if ($g -ne 'total') { $inGroup = @($at | Where-Object { $_.Group -eq $g }) }
            foreach ($m in $metrics) {
                $sum = 0.0
                foreach ($r in $inGroup) { $sum += [double]$r.$m }
                [void]$perSample["$g.$m"].Add($sum)
            }
        }
    }

    ''
    'Summary over {0} samples (CPU % of {1})' -f $byTime.Count, $Basis
    '{0,-6} {1,-7} {2,9} {3,9} {4,9} {5,9}' -f 'group', 'metric', 'min', 'median', 'p95', 'max'
    foreach ($g in $groups) {
        foreach ($m in $metrics) {
            $sorted = [double[]]@($perSample["$g.$m"] | Sort-Object)
            '{0,-6} {1,-7} {2,9:N1} {3,9:N1} {4,9:N1} {5,9:N1}' -f $g, $m, $sorted[0], (Get-Percentile $sorted 50), (Get-Percentile $sorted 95), $sorted[-1]
        }
    }

    ''
    'Median per process name (count, CPU %, working set MB, private MB)'
    '{0,-6} {1,-24} {2,5} {3,7} {4,9} {5,9}' -f 'group', 'name', 'count', 'cpu', 'ws_mb', 'priv_mb'
    $byName = $Rows | Group-Object Group, Name
    foreach ($n in ($byName | Sort-Object Name)) {
        $first = $n.Group[0]
        $vals = @{}
        foreach ($m in @('Count', 'Cpu', 'WsMb', 'PrivMb')) {
            # Samples where the name is absent count as zero, so medians stay comparable across names.
            $list = New-Object System.Collections.ArrayList
            foreach ($r in $n.Group) { [void]$list.Add([double]$r.$m) }
            for ($i = $list.Count; $i -lt $byTime.Count; $i++) { [void]$list.Add(0.0) }
            $vals[$m] = Get-Percentile ([double[]]@($list | Sort-Object)) 50
        }
        '{0,-6} {1,-24} {2,5:N0} {3,7:N1} {4,9:N1} {5,9:N1}' -f $first.Group, $first.Name, $vals['Count'], $vals['Cpu'], $vals['WsMb'], $vals['PrivMb']
    }
}

if ($PSCmdlet.ParameterSetName -eq 'Summary') {
    $rows = @(Import-Csv $FromCsv)
    if ($rows.Count -eq 0) { throw "No rows in $FromCsv" }
    # The file does not store the recording machine's core count, so name the basis without guessing it.
    Write-Summary $rows "the recording machine's logical cores"
    return
}

if ($Seconds -lt $Interval) { throw "-Seconds ($Seconds) must be at least -Interval ($Interval)" }
$skip = @($Exclude) + @($PID)
if ($Csv) {
    if (Test-Path $Csv) { throw "$Csv exists; record each run to a new file so runs are never mixed" }
    $dir = Split-Path -Parent $Csv
    if ($dir -and -not (Test-Path $dir)) { [void](New-Item -ItemType Directory -Path $dir) }
    Set-Content -Path $Csv -Value 'Time,Group,Name,Count,Cpu,WsMb,PrivMb' -Encoding UTF8
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
                Time = $wall.ToString('yyyy-MM-ddTHH:mm:ss.fff'); Group = $a.Group; Name = $a.Name; Count = $a.Count
                Cpu = [math]::Round($a.Cpu100ns / ($span * $Cores) * 100, 2)
                WsMb = [math]::Round($a.Ws / 1MB, 1); PrivMb = [math]::Round($a.Priv / 1MB, 1)
            }
        }
        $sampleRows = @($sampleRows)
        foreach ($r in $sampleRows) { [void]$all.Add($r) }
        if ($Csv) { $sampleRows | ConvertTo-Csv -NoTypeInformation | Select-Object -Skip 1 | Add-Content -Path $Csv -Encoding UTF8 }

        if (-not $Quiet) {
            $s = @{}
            foreach ($k in @('app.cpu', 'app.ws', 'app.priv', 'child.cpu', 'child.ws', 'child.priv')) { $s[$k] = 0.0 }
            foreach ($r in $sampleRows) {
                $s["$($r.Group).cpu"] += $r.Cpu
                $s["$($r.Group).ws"] += $r.WsMb
                $s["$($r.Group).priv"] += $r.PrivMb
            }
            '{0,-8} {1,4} | {2,6:N1} {3,8:N0} {4,8:N0} | {5,6:N1} {6,8:N0} {7,8:N0} | {8,6:N1} {9,8:N0} {10,8:N0}' -f $stamp, $tree.Count,
                $s['app.cpu'], $s['app.ws'], $s['app.priv'], $s['child.cpu'], $s['child.ws'], $s['child.priv'],
                ($s['app.cpu'] + $s['child.cpu']), ($s['app.ws'] + $s['child.ws']), ($s['app.priv'] + $s['child.priv'])
        }
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
