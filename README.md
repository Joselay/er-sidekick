# er-editor

Live memory editor for Elden Ring. Two commands, changes show up in-game
instantly, no save-file editing, no restart.

```
$ er-editor read
  Vigor          12
  Strength       23
  Dexterity      19
  Level          25
  Runes (held)   594
  ...

$ er-editor set runes=+100000
=== Stats updated ===
  Runes          594 -> 100594
```

Supported fields: `vigor`, `mind`, `endurance`, `strength`, `dexterity`,
`intelligence`, `faith`, `arcane`, `level`, `runes`, `runes_total`.

Values may be prefixed with `+` / `-` for relative edits. `level` is
auto-corrected to match the stat sum.

## Build

```
cargo build --release
```

Windows-only. Requires Elden Ring 2.06.0 with EAC disabled (e.g. via Seamless
Coop) and a character loaded in the world.

## Optional: driven by Claude Code

This project was designed to be invoked by an LLM agent — you ask in natural
language ("give me 10k runes", "raise Dex to 40"), the agent runs the CLI,
parses the output, reports back. See `CLAUDE.md` for the project rules the
agent follows. You can also just use the CLI directly like any other tool.

## Optional: Valorant / Vanguard safety

**Only applies if you have Valorant installed on the same machine.** Riot
Vanguard is a kernel-level anti-cheat that scans for cross-process memory
reads/writes (which is what this tool does). Running `er-editor` while
Vanguard is active is a potential Valorant account risk.

If that's you, stop Vanguard first:

```powershell
sc.exe stop vgc
sc.exe stop vgk
```

Reboot before launching Valorant again. The included
`.claude/hooks/preflight-vanguard.ps1` auto-blocks `er-editor` invocations
under Claude Code if Vanguard is still active. If you don't have Valorant
installed, the hook passes through silently.

## Credits

Pointer map, AOB pattern, and chain layout from
[veeenu/eldenring-practice-tool](https://github.com/veeenu/eldenring-practice-tool).
