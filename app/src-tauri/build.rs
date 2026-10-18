fn main() {
    // On Windows the app (and therefore the engine it spawns) must run elevated so the
    // engine can open raw physical drives. Embed a manifest requesting administrator.
    #[cfg(windows)]
    {
        let windows = tauri_build::WindowsAttributes::new()
            .app_manifest(include_str!("windows-app-manifest.xml"));
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("failed to run tauri-build");
    }
    #[cfg(not(windows))]
    tauri_build::build();
}
