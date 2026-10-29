# The pure parts of the sampler live apart from its loop so tests can feed them synthetic data.
function Get-TreeRow {
    param([object[]]$Snapshot, [string]$RootName, [int[]]$Skip)

    $rootFile = ($RootName -replace '\.exe$', '') + '.exe'
    # ConPTY hosts started by the app belong to the terminal, not to the program running in it.
    $ptyHosts = @('openconsole.exe', 'conhost.exe')
    $byParent = @{}
    $byId = @{}
    foreach ($p in $Snapshot) {
        $byId[[int]$p.ProcessId] = $p
        $parent = [int]$p.ParentProcessId
        if (-not $byParent.ContainsKey($parent)) { $byParent[$parent] = New-Object System.Collections.ArrayList }
        [void]$byParent[$parent].Add($p)
    }

    $rows = New-Object System.Collections.ArrayList
    $queue = New-Object System.Collections.Queue
    foreach ($p in $Snapshot) {
        if ($p.Name -ine $rootFile) { continue }
        $parent = $byId[[int]$p.ParentProcessId]
        # A root is a process of that name whose parent is not one too, so helpers are not counted twice.
        if ($null -ne $parent -and $parent.Name -ieq $rootFile) { continue }
        $queue.Enqueue(@($p, 'app'))
    }

    # A same-named process under a differently named parent is both a root and a descendant; count it once.
    $seen = @{}
    while ($queue.Count -gt 0) {
        $item = $queue.Dequeue()
        $p = $item[0]
        $group = $item[1]
        if ($Skip -contains [int]$p.ProcessId -or $seen.ContainsKey([int]$p.ProcessId)) { continue }
        $seen[[int]$p.ProcessId] = $true
        [void]$rows.Add([pscustomobject]@{ Process = $p; Group = $group })
        if (-not $byParent.ContainsKey([int]$p.ProcessId)) { continue }
        foreach ($c in $byParent[[int]$p.ProcessId]) {
            # A PID can be reused by a process started after its parent exited, so check start order.
            if ($c.CreationDate -lt $p.CreationDate) { continue }
            $childGroup = 'child'
            if ($group -eq 'app' -and ($c.Name -ieq $rootFile -or $ptyHosts -contains $c.Name.ToLower())) { $childGroup = 'app' }
            $queue.Enqueue(@($c, $childGroup))
        }
    }
    # The comma keeps a one-process tree an array, so callers can rely on .Count under strict mode.
    return , $rows.ToArray()
}

function Get-SampleStamp {
    param([datetime]$Wall)
    # Milliseconds keep two samples taken in the same second apart when the summary groups by stamp.
    return $Wall.ToString('yyyy-MM-ddTHH:mm:ss.fff')
}

function Format-SampleLine {
    param([object[]]$Rows, [string]$Clock, [int]$ProcessCount)
    $s = @{}
    foreach ($k in @('app.cpu', 'app.ws', 'app.priv', 'child.cpu', 'child.ws', 'child.priv')) { $s[$k] = 0.0 }
    foreach ($r in $Rows) {
        $s["$($r.Group).cpu"] += [double]$r.Cpu
        $s["$($r.Group).ws"] += [double]$r.WsMb
        $s["$($r.Group).priv"] += [double]$r.PrivMb
    }
    return '{0,-8} {1,4} | {2,6:N1} {3,8:N0} {4,8:N0} | {5,6:N1} {6,8:N0} {7,8:N0} | {8,6:N1} {9,8:N0} {10,8:N0}' -f $Clock, $ProcessCount,
        $s['app.cpu'], $s['app.ws'], $s['app.priv'], $s['child.cpu'], $s['child.ws'], $s['child.priv'],
        ($s['app.cpu'] + $s['child.cpu']), ($s['app.ws'] + $s['child.ws']), ($s['app.priv'] + $s['child.priv'])
}

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
