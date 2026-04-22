#[cfg(not(target_arch = "wasm32"))]
fn main() {
    leetcodedaily::run();
}

#[cfg(target_arch = "wasm32")]
fn main() {}
