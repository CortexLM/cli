//! Renders the Cortex Code session demo to ANSI frames.
//!
//! The frames are the input to `scripts/render-demo-gif.sh`, which produces the
//! README banner at `docs/media/intro.gif`.
//!
//! ```bash
//! cargo run -p cortex-tui-capture --bin generate_tui_demo -- --output target/tui-demo
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use cortex_tui_capture::demo::{self, DemoConfig};

fn print_help() {
    println!(
        r#"Cortex Code demo recorder - render the session view to ANSI frames

USAGE:
    generate_tui_demo [OPTIONS]

OPTIONS:
    -o, --output <DIR>    Output directory (default: {default_dir})
    -w, --width <COLS>    Terminal width (default: {default_width})
        --height <ROWS>   Terminal height (default: {default_height})
        --fps <FPS>       Playback rate recorded in the manifest (default: {default_fps})
    -h, --help            Show this help message
"#,
        default_dir = demo::DEFAULT_OUTPUT_DIR,
        default_width = demo::DEFAULT_WIDTH,
        default_height = demo::DEFAULT_HEIGHT,
        default_fps = demo::DEFAULT_FPS,
    );
}

fn parse_args() -> Result<Option<DemoConfig>, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut config = DemoConfig::default();

    let mut i = 1;
    while i < args.len() {
        let flag = args[i].as_str();
        let mut value = || -> Result<String, String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("Missing value for {flag}"))
        };

        match flag {
            "-h" | "--help" => return Ok(None),
            "-o" | "--output" => config.output_dir = PathBuf::from(value()?),
            "-w" | "--width" => {
                config.width = value()?.parse().map_err(|_| "Invalid width".to_string())?;
            }
            "--height" => {
                config.height = value()?.parse().map_err(|_| "Invalid height".to_string())?;
            }
            "--fps" => {
                config.fps = value()?.parse().map_err(|_| "Invalid fps".to_string())?;
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
        i += 1;
    }

    Ok(Some(config))
}

fn main() -> ExitCode {
    let config = match parse_args() {
        Ok(Some(config)) => config,
        Ok(None) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("Error: {err}\n");
            print_help();
            return ExitCode::FAILURE;
        }
    };

    let recording = match demo::record(&config) {
        Ok(recording) => recording,
        Err(err) => {
            eprintln!("Failed to render the demo: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = recording.write_to_dir(&config.output_dir) {
        eprintln!("Failed to write the demo frames: {err}");
        return ExitCode::FAILURE;
    }

    println!(
        "Rendered {} frames ({}x{}, {:.1}s at {} fps) to {}",
        recording.frames.len(),
        recording.width,
        recording.height,
        recording.duration_secs(),
        recording.fps,
        config.output_dir.display()
    );

    ExitCode::SUCCESS
}
