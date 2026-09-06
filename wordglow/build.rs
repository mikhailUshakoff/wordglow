fn main() {
    // Build scripts always run on the host, so `cfg(target_arch)` here would
    // reflect the host, not the target — check TARGET explicitly instead to
    // skip the native Tauri codegen when Trunk is compiling this crate to wasm.
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("wasm32") {
        tauri_build::build();
    }
}
