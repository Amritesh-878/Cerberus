# The tree walk lives apart from the sampling loop so tests can feed it synthetic snapshots.
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
