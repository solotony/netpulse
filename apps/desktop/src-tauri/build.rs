fn main() {
    // Dock/window icons are embedded by `generate_context!` only when this crate
    // recompiles. Cargo does not watch those files on its own, so a changed
    // `.icns` would otherwise leave macOS showing the previous icon.
    for path in ["icons/icon.icns", "icons/icon.png", "icons/icon.ico"] {
        println!("cargo:rerun-if-changed={path}");
        println!("cargo:rustc-env=NETPULSE_ICON_{}={}", env_key(path), icon_stamp(path));
    }
    tauri_build::build()
}

fn env_key(path: &str) -> String {
    path.rsplit('.')
        .next()
        .unwrap_or("icon")
        .to_ascii_uppercase()
}

fn icon_stamp(path: &str) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return "missing".to_string();
    };
    let secs = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{}-{secs}", meta.len())
}
