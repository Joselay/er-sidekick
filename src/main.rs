use std::env;
use std::process;

#[cfg(windows)]
mod mem;

fn usage_and_exit() -> ! {
    eprintln!("usage:");
    eprintln!("  er-editor read [--json]");
    eprintln!("  er-editor set  <key=value,key=value,...>");
    eprintln!();
    eprintln!("  attaches to eldenring.exe and reads/writes stats in real time.");
    eprintln!("  requires EAC to be disabled (e.g. Seamless Coop) and a character");
    eprintln!("  loaded in the world.");
    eprintln!();
    eprintln!("  values may be prefixed with + or - for relative changes.");
    eprintln!("  stat keys: vigor, mind, endurance, strength, dexterity,");
    eprintln!("             intelligence, faith, arcane, level, runes, runes_total");
    process::exit(1);
}

fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("er-editor requires Windows (reads eldenring.exe memory)");
        process::exit(1);
    }

    #[cfg(windows)]
    {
        let args: Vec<String> = env::args().collect();
        if args.len() < 2 { usage_and_exit(); }
        match args[1].as_str() {
            "read" => match &args[2..] {
                [] => mem::run_read(false),
                [flag] if flag == "--json" => mem::run_read(true),
                _ => usage_and_exit(),
            },
            "set" => match &args[2..] {
                [edits] => mem::run_set(edits),
                _ => usage_and_exit(),
            },
            _ => usage_and_exit(),
        }
    }
}
