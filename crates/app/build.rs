//! Embeds Recap's icon and version information in recap.exe (Windows): zgx's
//! windows, the taskbar, Alt+Tab, Explorer and the Start menu shortcut take
//! the executable's first icon (assets/icon/recap.ico, from
//! tools/gen_icon.py).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/recap.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/icon/recap.ico")
        .set("ProductName", "Recap")
        .set("FileDescription", "Recap")
        .set("CompanyName", "Recap");
    if let Err(error) = resource.compile() {
        // No resource compiler: the app still builds, with Windows' default icon.
        println!("cargo:warning=recap: no icon embedded: {error}");
    }
}
