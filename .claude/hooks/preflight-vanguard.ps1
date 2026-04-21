# Vanguard pre-flight safety hook for er-editor.
#
# Reads PreToolUse JSON from stdin. If the tool_input.command invokes
# er-editor (any subcommand), verifies that Valorant/Vanguard is fully
# stopped. Blocks (exit 2) if anything is still active; allows (exit 0)
# otherwise. Non-matching commands are passed through unconditionally.
#
# All er-editor subcommands read/write eldenring.exe memory, which is
# the category of behavior Vanguard scans for — so every invocation is
# gated, not just a specific subcommand.

$ErrorActionPreference = 'Stop'

try {
    $raw = [Console]::In.ReadToEnd()
    if (-not $raw) { exit 0 }
    $payload = $raw | ConvertFrom-Json
    $cmd = $payload.tool_input.command
    if (-not $cmd) { exit 0 }
} catch {
    # If we can't parse, don't block — fail-open to avoid locking the user out
    # if the hook payload shape changes.
    exit 0
}

# Match any token ending in `er-editor` or `er-editor.exe`. This catches
# absolute/relative paths (e.g. `./target/release/er-editor.exe read`).
$cleaned = $cmd -replace '["\'']', ' '
$tokens = ($cleaned -split '\s+') | Where-Object { $_ -ne '' }

$isErEditor = $false
foreach ($t in $tokens) {
    if ($t.ToLower() -match 'er-editor(\.exe)?$') {
        $isErEditor = $true
        break
    }
}
if (-not $isErEditor) { exit 0 }

# --- Vanguard/Valorant safety check ------------------------------------

$problems = New-Object System.Collections.Generic.List[string]

$riotProcs = Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^(VALORANT|Riot|vgc|vgtray|vanguard)' }
if ($riotProcs) {
    $names = ($riotProcs | Select-Object -ExpandProperty Name -Unique) -join ', '
    $problems.Add("running Riot/Valorant processes: $names")
}

$vgk = Get-Service vgk -ErrorAction SilentlyContinue
if ($vgk -and $vgk.Status -ne 'Stopped') {
    $problems.Add("vgk service status = $($vgk.Status) (must be Stopped)")
}
$vgc = Get-Service vgc -ErrorAction SilentlyContinue
if ($vgc -and $vgc.Status -ne 'Stopped') {
    $problems.Add("vgc service status = $($vgc.Status) (must be Stopped)")
}

if ($problems.Count -eq 0) { exit 0 }

[Console]::Error.WriteLine('')
[Console]::Error.WriteLine('BLOCKED: er-editor refused — Vanguard/Valorant is still active.')
[Console]::Error.WriteLine('Running this while Vanguard is loaded risks flagging the Valorant account.')
[Console]::Error.WriteLine('')
foreach ($p in $problems) { [Console]::Error.WriteLine("  - $p") }
[Console]::Error.WriteLine('')
[Console]::Error.WriteLine('To clear, from an admin shell:')
[Console]::Error.WriteLine('  Get-Process | Where-Object { $_.Name -match ''VALORANT|Riot|vgc|vgtray|vanguard'' } | Stop-Process -Force')
[Console]::Error.WriteLine('  sc.exe stop vgc')
[Console]::Error.WriteLine('  sc.exe stop vgk')
[Console]::Error.WriteLine('Then retry the command.')
exit 2
