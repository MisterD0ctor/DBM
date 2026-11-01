/// Where the translations live: `<code>/LC_MESSAGES/dbm-player.po` under
/// it. `dbm-lang` keeps them, and reads the Rust side's words out of them.
const PO: &str = "../lang/po";

fn main() {
    // The words in `app.slint` are bundled by Slint itself, which picks the
    // language off the system and can be told another — see `dbm_lang`.
    let config = slint_build::CompilerConfiguration::new().with_bundled_translations(PO);
    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile app.slint");
    pass_on_the_keys();
    stamp_the_executable();
}

/// Hand the code the keys it was built with, if any — see `keys/README.md`.
///
/// The folder is watched rather than the file: a file that does not exist
/// would have Cargo run this, and rebuild the interface, on every build.
fn pass_on_the_keys() {
    println!("cargo:rerun-if-env-changed=DBM_OPENSUBTITLES_KEY");
    println!("cargo:rerun-if-changed=keys");
    let key = std::env::var("DBM_OPENSUBTITLES_KEY")
        .ok()
        .or_else(|| std::fs::read_to_string("keys/opensubtitles").ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if let Some(key) = key {
        println!("cargo:rustc-env=DBM_OPENSUBTITLES_KEY={key}");
    }
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
