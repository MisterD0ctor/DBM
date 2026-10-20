//! ffmpeg, which is not libmpv and not a dependency of this crate.
//!
//! Four things shell out to it: the seek preview, which cuts a sheet of
//! frames out of a film; the playlist scan, which asks each file how long it
//! is and what it is called; the subtitle sync, which listens to a film's
//! audio; and the search for an episode's title sequence and credits, which
//! listens to two. Without it each degrades rather than fails (the preview to
//! the timestamp alone, the rows to what mpv has said, titles and credits to
//! the chapters), so it is mentioned once.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Where ffmpeg is, if anywhere.
///
/// `DBM_FFMPEG` overrides; then the copy this app ships with; then whatever
/// the `PATH` turns up. The shipped copy comes before `PATH`, or a machine
/// with its own ffmpeg would quietly use a different build.
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
/// Four places can supply it (the override, beside the executable, the
/// vendor directory, the `PATH`), so "the thumbnails are missing" needs this
/// to be answerable.
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
/// subtitle sync or the title search listens to. The player draws at the
/// display's rate and mpv decodes against the clock, so a wait for a core
/// shows on screen, while a thumbnail sheet arriving later doesn't.
///
/// On Linux the niceness is set once the process has started, because
/// setting it before takes a `pre_exec` hook, which makes the standard
/// library fork the whole player rather than spawn. The moments ffmpeg runs
/// at full priority are spent loading itself.
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
