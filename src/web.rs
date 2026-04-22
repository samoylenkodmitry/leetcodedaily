#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen(start)]
pub fn start() {
    spawn_local(async {
        if let Err(error) = crate::ui::run_web().await {
            panic!("web launch failed: {error:?}");
        }
    });
}
