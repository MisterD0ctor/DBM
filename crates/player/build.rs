fn main() {
    slint_build::compile("ui/app.slint").expect("failed to compile app.slint");
    stamp_the_executable();
}

/// The icon and version block Windows reads off the file itself.
///
/// Separate from the window icon, which Slint sets from a PNG at runtime:
/// this is the one Explorer, the taskbar and the installer's Add/Remove entry
/// show, and it has to be compiled into the binary as a resource.
///
/// Deliberately not fatal. It needs `rc.exe` from the Windows SDK, and a
/// machine without one should still get a working player — with a default
/// icon, which is a cosmetic loss rather than a broken build.
#[cfg(windows)]
fn stamp_the_executable() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("icons/icon.ico");
    resource.set("ProductName", "Death by MPV");
    resource.set("FileDescription", "Death by MPV");
    if let Err(e) = resource.compile() {
        println!("cargo:warning=could not embed the icon or version block: {e}");
    }
}

#[cfg(not(windows))]
fn stamp_the_executable() {}
