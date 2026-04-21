# er-sidekick — project rules for Claude Code

## What this project is

Rust CLI that reads and edits Elden Ring saves (`read` / `patch`) and — on
Windows — attaches to the running game to edit stats in real time (`live`).
Designed to be driven by Claude Code as an agent: user asks in natural
language, Claude Code invokes the CLI, parses output, and calls the right
subcommand.

## CRITICAL: Vanguard safety protocol for `live` mode

This machine has **Riot Vanguard installed** (for Valorant). `live` mode
does cross-process `ReadProcessMemory` / `WriteProcessMemory` on
`eldenring.exe`, which is the category of behavior Vanguard scans for.
Running `live` while Vanguard is active is a potential Valorant account
ban risk.

**Before running ANY `live` subcommand (`live read`, `live set`, or a
future `live *`), always run the pre-flight check and stop anything Riot
related. Do not skip. Do not assume the previous session left things
clean — always re-verify.**

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

If any check fails, **abort and tell the user** — do not attempt `live`
commands until state is clean. Offer to retry the stop commands from an
admin shell if needed.

### After `live` use

- Tell the user: to play Valorant again, reboot (Vanguard reloads at
  boot). `sc start vgk` alone may not satisfy Vanguard's boot-integrity
  check; reboot is the reliable path.
- Never launch Valorant in the same session as `live` mode without a
  reboot in between.

### What does NOT need the pre-flight

- `read` (save file parsing) — no process access, safe to run anytime.
- `patch` (save file editing) — no process access, safe to run anytime.
- Building, testing, non-live code changes.

Only the `live` subcommand touches another process's memory.

## Game context

- Game install: `C:\Program Files\Elden Ring\Game\eldenring.exe`
- Game version: **2.06.0** (confirmed; supported by veeenu's pointer map)
- Launched via **Seamless Coop** (`ersc_launcher.exe`) — EAC is disabled
  at launch by the mod. Save extension is `.co2` not `.sl2` (see
  `Game\SeamlessCoop\ersc_settings.ini`, line `save_file_extension`).
- Default save location: `%APPDATA%\EldenRing\<steamid>\ER0000.co2`

## Agent workflow

- User asks in natural language ("change Dex 25→30", "how are my stats?",
  "give me 5000 runes").
- Prefer `read --json` / `live read --json` for parsing, fall back to the
  human-readable form only when talking TO the user.
- For any edit: read current state first, compute the diff, then apply —
  so you can report "X → Y" rather than just "set to Y".
- For `live set`, the pointer chain requires a character loaded into the
  world (not title / character select). If you get
  `"pointer chain walk failed"`, tell the user to load a character first.

## Pointer / offset facts (do not re-derive)

Memory layout (via veeenu/eldenring-practice-tool, ER 2.06.x):
- GameDataMan AOB: `48 8B 05 ?? ?? ?? ?? 48 85 C0 74 05 48 8B 40 58 C3 C3`
  (disp32 @ +3, next-ip @ +7)
- CharacterStats addr = `*(*GameDataMan + 0x8) + 0x3c`
- Struct layout: 8×i32 stats packed, 3×u32 pad, level i32, runes i32,
  runes_total i32 (56 bytes total).

Save-file layout (different from memory — do not cross-use):
- See comments at top of `src/save.rs` for the authoritative offsets.

## Build / test

- `cargo build --release` — produces `target/release/er-sidekick.exe`
- No automated tests yet. For `live`, end-to-end requires the game
  running with a character loaded.
