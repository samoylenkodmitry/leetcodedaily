#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = leetcodedaily::run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
