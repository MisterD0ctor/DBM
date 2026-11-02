/// Hand the code the keys it was built with, if any — see `keys/README.md`.
/// Here, beside the code that uses them, since a build script's `rustc-env`
/// reaches only its own crate.
///
/// The folder is watched rather than the file: a file that does not exist
/// would have Cargo run this, and rebuild the crate, on every build.
fn main() {
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
