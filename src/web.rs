#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen(start)]
pub fn start() {
    spawn_local(async {
        match crate::ui::run_web().await {
            Ok(()) => mark_ready(),
            Err(error) => {
                let message = js_error_message(&error);
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "web launch failed: {message}"
                )));
                show_error(&format!("Unable to start the app: {message}"));
            }
        }
    });
}

fn js_error_message(error: &JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

fn mark_ready() {
    set_root_state("ready");
    hide_element("app-status");
    hide_element("app-error");
}

fn show_error(message: &str) {
    set_root_state("error");
    hide_element("app-status");
    if let Some(element) = element_by_id("app-error") {
        element.set_text_content(Some(message));
        let _ = element.remove_attribute("hidden");
    }
}

fn hide_element(id: &str) {
    if let Some(element) = element_by_id(id) {
        let _ = element.set_attribute("hidden", "");
    }
}

fn set_root_state(state: &str) {
    let Some(document) = document() else {
        return;
    };
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute("data-app-state", state);
    }
}

fn element_by_id(id: &str) -> Option<web_sys::Element> {
    document()?.get_element_by_id(id)
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}
