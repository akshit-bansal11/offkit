fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("cupertino".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI failed to compile");

    // A build script runs on the host, so the target comes from the environment, not `cfg!`.
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        embed_windows_resources();
    }
}

/// Gives offkit.exe its icon and product name (Explorer, the taskbar and the installer show them).
#[cfg(windows)]
fn embed_windows_resources() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    winresource::WindowsResource::new()
        .set_icon("assets/icon.ico")
        .set("ProductName", "Offkit")
        .compile()
        .expect("couldn't embed the Windows resources");
}

// `winresource` is only a build dependency on Windows hosts, so a build that targets Windows
// from another OS ships without the embedded icon.
#[cfg(not(windows))]
fn embed_windows_resources() {}
