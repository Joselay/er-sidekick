use std::env;
use std::process;

use er_sidekick::{patch, read};

fn usage_and_exit() -> ! {
    eprintln!("usage:");
    eprintln!("  er-sidekick read  [--json] <save_file>");
    eprintln!("  er-sidekick patch <input_save> <output_save>");
    eprintln!("  er-sidekick live  read [--json]");
    eprintln!("  er-sidekick live  set <key=value,key=value,...>");
    eprintln!();
    eprintln!("  live editing attaches to eldenring.exe and changes stats in");
    eprintln!("  real time. requires EAC to be disabled (e.g. Seamless Coop).");
    eprintln!("  values may be prefixed with + or - for relative changes.");
    eprintln!("  stat keys: vigor, mind, endurance, strength, dexterity,");
    eprintln!("             intelligence, faith, arcane, level, runes, runes_total");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { usage_and_exit(); }
    match args[1].as_str() {
        "read" => {
            let rest = &args[2..];
            let (as_json, path) = match rest {
                [p] => (false, p.as_str()),
                [flag, p] if flag == "--json" => (true, p.as_str()),
                _ => usage_and_exit(),
            };
            read::run(path, as_json);
        }
        "patch" => {
            if args.len() != 4 { usage_and_exit(); }
            patch::run(&args[2], &args[3]);
        }
        "live" => {
            #[cfg(windows)]
            {
                use er_sidekick::live;
                let rest = &args[2..];
                match rest {
                    [sub] if sub == "read" => live::run_read(false),
                    [sub, flag] if sub == "read" && flag == "--json" => live::run_read(true),
                    [sub, edits] if sub == "set" => live::run_set(edits),
                    _ => usage_and_exit(),
                }
            }
            #[cfg(not(windows))]
            {
                eprintln!("live editing is only supported on Windows");
                process::exit(1);
            }
        }
        other => {
            if args.len() == 3 {
                patch::run(other, &args[2]);
            } else {
                usage_and_exit();
            }
        }
    }
}
