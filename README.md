# er-editor

A live memory editor for Elden Ring — driven by natural language, not buttons.

You talk to Claude Code. Claude Code talks to this binary. This binary talks to
`eldenring.exe`. Your stats change instantly in-game, no save-file editing, no
restart.

```
You:    "give me 100000 runes"
Claude: er-editor set runes=+100000
Game:   runes counter updates live
```

## What it does

Attaches to a running `eldenring.exe` on Windows, AOB-scans for the
`GameDataMan` pointer, walks the CharacterStats pointer chain, and reads/writes
the in-memory stat struct via `ReadProcessMemory` / `WriteProcessMemory`.

Supported fields: `vigor`, `mind`, `endurance`, `strength`, `dexterity`,
`intelligence`, `faith`, `arcane`, `level`, `runes`, `runes_total`.

Everything else (inventory, flags, teleport, scaling) is future scope — the
pointer map from [veeenu/eldenring-practice-tool](https://github.com/veeenu/eldenring-practice-tool)
covers those when you want them.

## Why it's built this way

Traditional trainers have a GUI with hotkeys. This doesn't. The UX is a
conversation:

```
           ┌───────────────────────┐
  you ───► │      Claude Code      │ ◄── interprets intent, plans edits
           └──────────┬────────────┘
                      │ invokes CLI
                      ▼
           ┌───────────────────────┐
           │       er-editor       │ ◄── deterministic executor
           └──────────┬────────────┘
                      │ RPM / WPM
                      ▼
           ┌───────────────────────┐
           │    eldenring.exe      │
           └───────────────────────┘
```

All intelligence lives in the LLM. The binary is small, auditable, and does one
thing: translate CLI args into bytes written at the right addresses. Two
commands total.

## CLI

```
er-editor read [--json]
er-editor set  <key=value,key=value,...>
```

Values may be prefixed with `+` or `-` for relative edits.

### Examples

```
$ er-editor read
  Vigor          12
  Mind           11
  Endurance      13
  Strength       23
  Dexterity      19
  Intelligence   10
  Faith           8
  Arcane          8
  Level          25
  Runes (held)  594
  Runes (memory)62577

$ er-editor set runes=+100000
=== Stats updated ===
  Runes          594 -> 100594

$ er-editor set vig=60,end=40
=== Stats updated ===
  Vigor           12 -> 60
  Endurance       13 -> 40
  Level           25 -> 100    # auto-corrected to match stat sum

$ er-editor read --json
{"vigor":60,"mind":11,"endurance":40,"strength":23,"dexterity":19, ...}
```

Level is auto-corrected to `sum(stats) − 79` on every edit so the game's own
save-write stays internally consistent.

## Requirements

- Windows
- Elden Ring **2.06.0** (via Seamless Coop; EAC disabled)
- A character loaded **in the world** (not title screen / character select)
- If you get `pointer chain walk failed`, the character isn't loaded yet
- Claude Code (or any agent / human) to drive the CLI

## Build

```
cargo build --release
# → target/release/er-editor.exe
```

One dependency (`windows` crate). Windows-only (`#[cfg(windows)]`).

## ⚠️ Vanguard / Valorant safety

If you have Valorant installed on the same machine, **always stop Vanguard
before running `er-editor`**. Vanguard is a kernel-level anti-cheat that
watches for exactly the category of behavior this tool performs (cross-process
memory reads/writes on game executables). Running `er-editor` while Vanguard
is active risks flagging your Valorant account.

```powershell
# Kill Riot/Valorant processes
Get-Process | Where-Object { $_.Name -match 'VALORANT|Riot|vgc|vgtray|vanguard' } |
    Stop-Process -Force -ErrorAction SilentlyContinue

# Stop Vanguard services (from an admin shell)
sc.exe stop vgc
sc.exe stop vgk

# Verify
Get-Service vgc, vgk | Format-Table Name, Status -AutoSize
```

Required state: `vgk` Stopped, `vgc` Stopped, no Riot/Valorant processes.

**Before launching Valorant again: reboot.** `sc start vgk` alone may not
satisfy Vanguard's boot-integrity check.

The included `.claude/hooks/preflight-vanguard.ps1` hook auto-blocks every
`er-editor` invocation under Claude Code unless these conditions are met.

## Project layout

```
er-editor/
├── src/
│   ├── main.rs             # CLI dispatch
│   └── mem.rs              # scan, pointer walk, RPM/WPM
├── Cargo.toml
├── CLAUDE.md               # agent rules + pointer facts
└── .claude/
    ├── settings.json       # PreToolUse hook registration
    └── hooks/
        └── preflight-vanguard.ps1
```

## Credits

Pointer map, AOB pattern, and pointer-chain layout lifted from
[veeenu/eldenring-practice-tool](https://github.com/veeenu/eldenring-practice-tool).
If you want features beyond stats, that's the canonical source.

## License

Unlicensed. Personal use.
