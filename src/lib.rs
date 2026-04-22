mod assets;
mod draft;
mod export;
mod ui;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub fn run() {
    ui::run();
}
