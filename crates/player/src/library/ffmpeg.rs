//! ffmpeg, which is not libmpv and not a dependency of this crate.
//!
//! Three things shell out to it: the seek preview, which cuts a sheet of
//! frames out of a film; the playlist scan, which asks each file how long it
//! is and what it is called without playing it; and the subtitle sync, which
//! listens to a film's audio. If it is not on the machine none of them
//! happens — the preview falls back to the timestamp alone, the rows to what
//! mpv itself has said, and the sync says it cannot — which is a feature
//! degrading, not a failure, and nothing reports it as one unasked after the
//! first mention.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

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

/// Without this every ffmpeg flashes a console window on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
/// The niceness `spawn_behind` gives a process: well behind the player, not
/// as far as idle, so the work still finishes on a machine that is busy with
/// something else.
#[cfg(not(windows))]
const BEHIND: libc::c_int = 10;

pub fn command(ffmpeg: &Path) -> Command {
    let mut cmd = Command::new(ffmpeg);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Start `cmd` below the player's own priority, so that when the two want the
/// same core the player gets it.
///
/// For work nobody is watching: the seek preview's frames, and the audio a
/// subtitle sync listens to. The player draws at the display's rate and mpv
/// decodes against the clock, so a moment's wait for a core shows on screen;
/// a thumbnail sheet or an answer that arrives a little later does not.
///
/// On Linux the niceness is set once the process has started, not before:
/// that would take a `pre_exec` hook, which makes the standard library fork
/// the whole player rather than spawn, and every page the renderer then
/// writes is copied while the child gets as far as `exec`. The moments
/// ffmpeg runs at full priority are spent loading itself.
pub fn spawn_behind(cmd: &mut Command) -> std::io::Result<Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    let child = cmd.spawn()?;
    // A process that cannot be reniced still does its work, only less
    // politely, so a refusal is not worth failing it over.
    #[cfg(not(windows))]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, child.id(), BEHIND);
    }
    Ok(child)
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
