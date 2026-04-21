# er-editor — project rules for Claude Code

## What this project is

Rust CLI that attaches to a running Elden Ring (`eldenring.exe`) on Windows
and reads/writes character stats in real time. Designed to be driven by
Claude Code as an agent: user asks in natural language ("change Dex 25→30",
"give me 5000 runes", "how are my stats?") and Claude Code invokes the CLI,
parses its output, and reports back.

## CRITICAL: Vanguard safety protocol

This machine has **Riot Vanguard installed** (for Valorant). Every
er-editor invocation does cross-process `ReadProcessMemory` /
`WriteProcessMemory` on `eldenring.exe`, which is the category of behavior
Vanguard scans for. Running er-editor while Vanguard is active is a
potential Valorant account ban risk.

**Before running ANY er-editor command, always run the pre-flight check
and stop anything Riot related. Do not skip. Do not assume the previous
session left things clean — always re-verify.** The PreToolUse hook in
`.claude/settings.json` enforces this automatically, but verify manually
before telling the user you are about to run something.

### Pre-flight check (run this every time)

```powershell
# 1. Kill any Riot / Valorant user-mode processes
Get-Process | Where-Object { $_.Name -match 'VALORANT|Riot|vgc|vgtray|vanguard' } |
    Stop-Process -Force -ErrorAction SilentlyContinue

# 2. Stop Vanguard services (kernel driver vgk + user service vgc)
sc.exe stop vgc  2>$null | Out-Null
sc.exe stop vgk  2>$null | Out-Null

# 3. Verify clean state
Get-Service vgc, vgk | Format-Table Name, Status -AutoSize
Get-Process | Where-Object { $_.Name -match 'VALORANT|Riot|vgc|vgtray|vanguard' } |
    Select-Object Id, Name
```

Required state before proceeding:
- `vgk` Status = `Stopped`
- `vgc` Status = `Stopped`
- No Riot/Valorant/Vanguard processes listed

If any check fails, **abort and tell the user** — do not attempt commands
until state is clean. Offer to retry the stop commands from an admin shell
if needed.

### After use

- Tell the user: to play Valorant again, reboot (Vanguard reloads at
  boot). `sc start vgk` alone may not satisfy Vanguard's boot-integrity
  check; reboot is the reliable path.
- Never launch Valorant in the same session as er-editor without a
  reboot in between.

### What does NOT need the pre-flight

- Building (`cargo build`), non-runtime code changes — no process access.

Everything else (any `er-editor read` / `er-editor set`) touches the
game's memory and must be gated.

## Game context

- Game install: `C:\Program Files\Elden Ring\Game\eldenring.exe`
- Game version: **2.06.0** (confirmed; supported by veeenu's pointer map)
- Launched via **Seamless Coop** (`ersc_launcher.exe`) — EAC is disabled
  at launch by the mod.

## Agent workflow

- User asks in natural language ("change Dex 25→30", "how are my stats?",
  "give me 5000 runes").
- Prefer `er-editor read --json` for parsing; use the human-readable
  form only when talking TO the user.
- For any edit: read current state first, compute the diff, then apply —
  so you can report "X → Y" rather than just "set to Y".
- The pointer chain requires a character loaded into the world (not
  title / character select). If you get `"pointer chain walk failed"`,
  tell the user to load a character first.

## CLI surface

```
er-editor read [--json]
er-editor set  <key=value,key=value,...>
```

Values may be prefixed with `+` or `-` for relative changes.
Stat keys: `vigor`, `mind`, `endurance`, `strength`, `dexterity`,
`intelligence`, `faith`, `arcane`, `level`, `runes`, `runes_total`.

## Pointer / offset facts (do not re-derive)

Memory layout (via veeenu/eldenring-practice-tool, ER 2.06.x):
- GameDataMan AOB: `48 8B 05 ?? ?? ?? ?? 48 85 C0 74 05 48 8B 40 58 C3 C3`
  (disp32 @ +3, next-ip @ +7)
- CharacterStats addr = `*(*GameDataMan + 0x8) + 0x3c`
- Struct layout: 8×i32 stats packed, 3×u32 pad, level i32, runes i32,
  runes_total i32 (56 bytes total).

## Build / test

- `cargo build --release` — produces `target/release/er-editor.exe`
- No automated tests. End-to-end requires the game running with a
  character loaded in world.
