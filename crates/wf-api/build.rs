// rust-embed needs the SPA folder at compile time. Without a web build (e.g. `cargo test`
// before `pnpm build`) an empty one is created, and the server answers that the UI is missing.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
    println!("cargo:rerun-if-changed=../../web/dist");
}
