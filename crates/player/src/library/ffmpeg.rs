//! ffmpeg, which is not libmpv and not a dependency of this crate.
//!
//! Two things shell out to it: the seek preview, which cuts a sheet of frames
//! out of a film, and the playlist scan, which asks each file how long it is
//! and what it is called without playing it. If it is not on the machine
//! neither happens — the preview falls back to the timestamp alone, and the
//! rows to what mpv itself has said — which is a feature degrading, not a
//! failure, and nothing reports it as one after the first mention.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where ffmpeg is, if anywhere.
///
/// `DBM_FFMPEG` overrides; then the copy this app ships with; then whatever
/// the `PATH` turns up. The shipped copy has to come before `PATH` or a
/// machine with its own ffmpeg would quietly use that one instead — a
/// different build, possibly a different decoder set, for no reason.
///
/// Two names because the vendored file keeps the target-triple suffix Tauri's
/// sidecar mechanism requires, while a packaged build places it plainly.
pub fn path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("DBM_FFMPEG") {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    let names: &[&str] = if cfg!(windows) {
        &["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe"]
    } else {
        &["ffmpeg", "ffmpeg-x86_64-unknown-linux-gnu"]
    };
    if let Some(shipped) = crate::paths::vendored(names) {
        return Some(shipped);
    }
    let name = names[0];
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(if cfg!(windows) { ';' } else { ':' })
        .map(|dir| Path::new(dir).join(name))
        .find(|p| p.is_file())
}

/// Name the ffmpeg actually in use, once per run.
///
/// Four things can supply it — the override, the copy beside the executable,
/// this crate's vendor directory, the `PATH` — so "the thumbnails are missing"
/// is otherwise a question with no way to answer it.
pub fn announce(ffmpeg: &Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| eprintln!("dbm: ffmpeg at {}", ffmpeg.display()));
}

pub fn command(ffmpeg: &Path) -> Command {
    let mut cmd = Command::new(ffmpeg);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // Without this every ffmpeg flashes a console window on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Ask ffmpeg to describe a file, and return what it said.
///
/// `-i` with no output makes ffmpeg read the container header, print what it
/// found and exit, without decoding a frame. **Blocking**: it spawns a
/// process and waits for it.
pub fn describe(ffmpeg: &Path, video: &Path) -> Option<String> {
    let out = command(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(video)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `  Duration: 01:23:45.67, start: ...`
pub fn parse_duration(text: &str) -> Option<f64> {
    let rest = text.split("Duration:").nth(1)?.trim_start();
    let clock = rest.split(',').next()?.trim();
    let mut parts = clock.split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let s: f64 = parts.next()?.trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_is_read_off_ffmpeg_report() {
        let text = "  Duration: 01:23:45.67, start: 0.000000, bitrate: 1234 kb/s";
        let seconds = parse_duration(text).unwrap();
        assert!((seconds - 5025.67).abs() < 0.01, "{seconds}");
    }
}
