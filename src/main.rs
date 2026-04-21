use std::env;
use std::process;

use er_sidekick::{patch, read};

fn usage_and_exit() -> ! {
    eprintln!("usage:");
    eprintln!("  er-sidekick read  [--json] <save_file>");
    eprintln!("  er-sidekick patch <input_save> <output_save>");
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
        other => {
            if args.len() == 3 {
                patch::run(other, &args[2]);
            } else {
                usage_and_exit();
            }
        }
    }
}
