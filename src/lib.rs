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
        Some(flag) if flag == "--capture-compose-preview" => {
            let draft_path = args
                .next()
                .ok_or_else(|| anyhow!("missing draft snapshot path"))?;
            let output_path = args
                .next()
                .ok_or_else(|| anyhow!("missing output image path"))?;
            if args.next().is_some() {
                return Err(anyhow!(
                    "unexpected extra arguments for compose capture mode"
                ));
            }
            ui::run_compose_capture_cli(Path::new(&draft_path), Path::new(&output_path))
        }
        Some(flag) => Err(anyhow!(
            "unknown command-line flag: {}",
            flag.to_string_lossy()
        )),
        None => Ok(ui::run()),
    }
}
