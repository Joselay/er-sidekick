# er-editor

Live memory editor for Elden Ring, driven by Claude Code. You ask in natural
language; stats change in-game instantly. No save-file editing, no restart.

```
You:    "give me 100000 runes"
Claude: er-editor set runes=+100000
Game:   runes counter updates live
```

Supported fields: `vigor`, `mind`, `endurance`, `strength`, `dexterity`,
`intelligence`, `faith`, `arcane`, `level`, `runes`, `runes_total`.

## Build

```
cargo build --release
```

Windows-only. Requires Elden Ring 2.06.0 with EAC disabled (e.g. Seamless
Coop) and a character loaded in the world.

## ⚠️ Vanguard

If Valorant is installed on the same machine, stop Vanguard before running:

```powershell
sc.exe stop vgc
sc.exe stop vgk
```

The included PreToolUse hook auto-blocks invocations otherwise. Reboot
before launching Valorant again.

## Credits

Pointer map from [veeenu/eldenring-practice-tool](https://github.com/veeenu/eldenring-practice-tool).
