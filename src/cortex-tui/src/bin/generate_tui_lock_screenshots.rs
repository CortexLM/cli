//! Generate visual-lock ANSI frames for rasterising to PNG.
//!
//! ```bash
//! cargo run -p cortex-tui --bin generate_tui_lock_screenshots -- \
//!   --width 120 --height 40 --output target/tui-lock/120x40
//! ```

use std::env;
use std::path::PathBuf;
use std::process;

use cortex_tui::lock_proof::write_lock_frames;
use cortex_tui::lock_v2::{validate_lock_v2_only_ids, write_lock_v2_frames};
use cortex_tui::lock_v2_batch56::{BATCH56_IDS, validate_batch56_only_ids, write_batch56_frames};

fn print_help() {
    println!(
        "\
Generate Cortex CLI visual-lock TUI frames (ANSI).

USAGE:
    generate_tui_lock_screenshots [OPTIONS]

OPTIONS:
    -o, --output <DIR>    Output directory (default: ./target/tui-lock)
    -w, --width <WIDTH>   Terminal width (default: 120)
    -h, --height <HEIGHT> Terminal height (default: 40)
    --v2                  Capture lock v2 scenes (real session chrome)
    --only <IDS>          Comma-separated lock v2 scene ids (with --v2)
    --batch56             Capture the Batch 56 boards (COR-447/448/449) instead
                          of the SPEC §7 pack; combine with --only for a subset
    --help                Show this help

Batch 56 ids ({}):
    {}
",
        BATCH56_IDS.len(),
        BATCH56_IDS.join(", ")
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut output = PathBuf::from("target/tui-lock");
    let mut width: u16 = 120;
    let mut height: u16 = 40;
    let mut v2 = false;
    let mut batch56 = false;
    let mut only_flag = false;
    let mut only: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => {
                print_help();
                process::exit(0);
            }
            "-o" | "--output" => {
                i += 1;
                output = PathBuf::from(args.get(i).unwrap_or_else(|| {
                    eprintln!("Missing value for --output");
                    process::exit(1);
                }));
            }
            "-w" | "--width" => {
                i += 1;
                width = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Invalid --width");
                    process::exit(1);
                });
            }
            "-h" | "--height" => {
                i += 1;
                height = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| {
                    eprintln!("Invalid --height");
                    process::exit(1);
                });
            }
            "--v2" => {
                v2 = true;
            }
            "--batch56" => {
                batch56 = true;
            }
            "--only" => {
                only_flag = true;
                i += 1;
                let raw = args.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("Missing value for --only");
                    process::exit(1);
                });
                only = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_help();
                process::exit(1);
            }
        }
        i += 1;
    }

    if only_flag && !v2 && !batch56 {
        eprintln!("--only requires --v2 or --batch56");
        process::exit(1);
    }
    if batch56 && v2 {
        eprintln!("--batch56 and --v2 are separate packs; pick one");
        process::exit(1);
    }

    let result = if batch56 {
        let ids: Vec<&str> = if only_flag {
            only.iter().map(String::as_str).collect()
        } else {
            BATCH56_IDS.to_vec()
        };
        if let Err(err) = validate_batch56_only_ids(&ids) {
            eprintln!("{err:#}");
            process::exit(1);
        }
        write_batch56_frames(&ids, width, height, &output)
    } else if v2 {
        if only_flag {
            let ids: Vec<&str> = only.iter().map(String::as_str).collect();
            if let Err(err) = validate_lock_v2_only_ids(&ids, width) {
                eprintln!("{err:#}");
                process::exit(1);
            }
            cortex_tui::lock_v2::write_lock_v2_id_frames(&ids, width, height, &output)
        } else {
            write_lock_v2_frames(width, height, &output)
        }
    } else {
        if only_flag {
            eprintln!("--only requires --v2 or --batch56");
            process::exit(1);
        }
        write_lock_frames(width, height, &output)
    };
    match result {
        Ok(manifest) => {
            println!("Wrote lock frames to {}", output.display());
            println!("Manifest: {}", manifest.display());
        }
        Err(err) => {
            eprintln!("{err:?}");
            process::exit(1);
        }
    }
}
