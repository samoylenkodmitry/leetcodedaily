#[cfg(not(target_arch = "wasm32"))]
use anyhow::{Result, anyhow};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

mod assets;
mod draft;
mod export;
#[cfg(not(target_arch = "wasm32"))]
mod publish;
mod ui;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub fn run() {
    ui::run();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_cli() -> Result<()> {
    let mut args = std::env::args_os();
    let _ = args.next();

    match args.next() {
        Some(flag) if flag == "--capture-app-screenshot" => {
            let first = args
                .next()
                .ok_or_else(|| anyhow!("missing output image path or capture width"))?;
            let second = args.next();
            match second {
                None => ui::run_app_capture_cli(Path::new(&first), None),
                Some(height) => {
                    let output_path = args
                        .next()
                        .ok_or_else(|| anyhow!("missing output image path"))?;
                    if args.next().is_some() {
                        return Err(anyhow!(
                            "unexpected extra arguments for app screenshot capture mode"
                        ));
                    }
                    let width = first
                        .to_string_lossy()
                        .parse::<u32>()
                        .map_err(|error| anyhow!("invalid capture width: {error}"))?;
                    let height = height
                        .to_string_lossy()
                        .parse::<u32>()
                        .map_err(|error| anyhow!("invalid capture height: {error}"))?;
                    ui::run_app_capture_cli(Path::new(&output_path), Some((width, height)))
                }
            }
        }
        Some(flag) => Err(anyhow!(
            "unknown command-line flag: {}",
            flag.to_string_lossy()
        )),
        None => {
            ui::run();
            Ok(())
        }
    }
}
