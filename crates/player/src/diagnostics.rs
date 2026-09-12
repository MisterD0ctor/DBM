//! Instrumentation, kept out of the way of the code it measures.
//!
//! All of it is opt-in and costs a branch when off:
//!
//! * `DBM_TRACE=1`     — periodic frame-rate and phase-timing report.
//! * `DBM_GPU_TIME=1`  — `glFinish` between pipeline passes, so the timings
//!                       are GPU work rather than how fast we queued it.
//!                       Serialises the pipeline; read the numbers, not the
//!                       frame rate, while it is on.
//! * `DBM_PROBE=1`     — one-shot pixel readbacks and a state dump. These
//!                       stall the pipeline outright.
//!
//! The phase timings earned their place: a 220ms-per-frame regression during
//! resize was invisible until the render path was broken down, and guessing
//! at it twice got the wrong answer both times.

use std::time::{Duration, Instant};

use crate::pipeline::Pipeline;
use crate::state::PlayerState;
use crate::tracks::TrackKind;
use crate::{pipeline::GlassPanel, tracks};

#[derive(Default)]
struct Phases {
    events: Duration,
    lists: Duration,
    panels: Duration,
    render: Duration,
}

pub struct Probe {
    trace: bool,
    probe: bool,
    /// Watches for the border being drawn against a video rect that does not
    /// match the frame under it. Checked every frame, because a resize causes
    /// it and a resize is over in a few.
    watch_border: bool,
    /// Worst gap yet between the rect the border used and the real one.
    worst_gap: f32,
    frames: u64,
    video_frames: u64,
    last_report: Instant,
    started: Instant,
    frames_at_report: u64,
    phases: Phases,
    last_panel_count: usize,
    /// Rects from the last frame, so a probe can sample a panel that is
    /// actually on screen rather than where one used to be.
    last_panels: Vec<([f32; 4], f32)>,
    /// One-shot latch. Firing on an exact frame number is fragile when the
    /// frame rate varies; this fires on the first frame past the mark.
    probed: bool,
    /// Watches for the border being drawn from a stale video rect, which is
    /// invisible while playing — the next frame redraws it — and sticks when
    /// paused. Only worth a line when the two disagree.
    warned_stale_rect: bool,
    /// The effect switches as of the last sample. Turning one off has to be
    /// checked in pixels — a stored `false` proves only that the click
    /// arrived, not that the pass stopped running — so a change here takes
    /// another reading.
    sampled_enables: Option<(bool, bool)>,
}

impl Probe {
    const REPORT_EVERY: Duration = Duration::from_secs(2);
    /// How long to wait for the frame count, before probing anyway.
    const PROBE_AFTER: Duration = Duration::from_secs(3);

    pub fn new() -> Self {
        Self {
            trace: std::env::var_os("DBM_TRACE").is_some(),
            probe: std::env::var_os("DBM_PROBE").is_some(),
            watch_border: std::env::var_os("DBM_BORDER_WATCH").is_some(),
            worst_gap: 0.0,
            frames: 0,
            video_frames: 0,
            last_report: Instant::now(),
            started: Instant::now(),
            frames_at_report: 0,
            phases: Phases::default(),
            last_panel_count: usize::MAX,
            last_panels: Vec::new(),
            probed: false,
            warned_stale_rect: false,
            sampled_enables: None,
        }
    }

    pub fn gl_info(&self, gl: &glow::Context) {
        if !self.trace {
            return;
        }
        use glow::HasContext;
        unsafe {
            eprintln!(
                "dbm: GL_VERSION={:?} GLSL={:?}",
                gl.get_parameter_string(glow::VERSION),
                gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION),
            );
        }
    }

    // --- phase timing ------------------------------------------------------
    // Each `mark` closes one phase and opens the next, so the call sites read
    // as a straight line through the frame.

    pub fn begin(&self) -> Instant {
        Instant::now()
    }

    pub fn mark_events(&mut self, t: Instant) -> Instant {
        self.phases.events += t.elapsed();
        Instant::now()
    }

    pub fn mark_lists(&mut self, t: Instant) -> Instant {
        self.phases.lists += t.elapsed();
        Instant::now()
    }

    pub fn mark_panels(&mut self, t: Instant) -> Instant {
        self.phases.panels += t.elapsed();
        Instant::now()
    }

    pub fn mark_render(&mut self, t: Instant) {
        self.phases.render += t.elapsed();
    }

    pub fn panels_changed(&mut self, panels: &[GlassPanel]) {
        self.last_panels.clear();
        self.last_panels
            .extend(panels.iter().map(|p| (p.rect, p.radius)));
        if !self.trace || panels.len() == self.last_panel_count {
            return;
        }
        self.last_panel_count = panels.len();
        eprintln!(
            "dbm: glass panels -> {} {:?}",
            panels.len(),
            panels.iter().map(|p| p.rect).collect::<Vec<_>>()
        );
    }

    pub fn frame_done(
        &mut self,
        new_frame: bool,
        pipeline: &Pipeline,
        gl: &glow::Context,
        player: &PlayerState,
        w: u32,
        h: u32,
    ) {
        self.frames += 1;
        if self.watch_border {
            self.watch_rect(player, w, h);
        }
        if new_frame {
            self.video_frames += 1;
        }

        if self.frames == 1 {
            let (cw, ch) = pipeline.composite().size();
            let (bw, bh) = pipeline.blur().size();
            eprintln!("dbm: first frame — composite {cw}x{ch}, blur {bw}x{bh}");
        }

        // Frames or seconds, whichever comes first. A paused player draws
        // only when something asks it to, so a frame count alone never
        // reaches the mark — and paused is the state most worth probing.
        if self.probe && !self.probed && (self.frames >= 90 || self.started.elapsed() > Self::PROBE_AFTER) {
            self.probed = true;
            self.dump(pipeline, gl, player);
            self.sampled_enables = Some(self.sample_enables(pipeline, gl, player));
        }

        // After the first reading, any switch flipping takes another. Only
        // once settled: the first frame after a toggle can still be showing
        // the previous composite.
        let enables = (pipeline.glass_enabled, pipeline.border_enabled);
        if self.probed && self.sampled_enables.is_some_and(|last| last != enables) {
            self.sampled_enables = Some(self.sample_enables(pipeline, gl, player));
        }

        // The border is drawn from a rect mpv reports asynchronously, so the
        // two are briefly out of step after every resize. Lasting past a
        // frame means something stopped redrawing it.
        let (want, drawn) = (player.video_rect(w, h), pipeline.border_rect());
        if want == drawn {
            self.warned_stale_rect = false;
        } else if !self.warned_stale_rect {
            self.warned_stale_rect = true;
            if self.trace {
                eprintln!("dbm: border rect {drawn:?} behind mpv's {want:?}");
            }
        }

        if self.trace && self.last_report.elapsed() >= Self::REPORT_EVERY {
            self.report(pipeline, w, h);
        }
    }

    fn report(&mut self, pipeline: &Pipeline, w: u32, h: u32) {
        let dt = self.last_report.elapsed().as_secs_f64();
        let (cw, ch) = pipeline.composite().size();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        eprintln!(
            "dbm: {:.0} draws/s, {:.0} video frames/s, window {w}x{h} targets {cw}x{ch} \
             | events {:.0}ms lists {:.0}ms panels {:.0}ms render {:.0}ms \
             | mpv {:.1} border+blur {:.1} glass {:.1} || rect {:.1} border {:.1} blur {:.1}",
            (self.frames - self.frames_at_report) as f64 / dt,
            self.video_frames as f64 / dt,
            ms(self.phases.events),
            ms(self.phases.lists),
            ms(self.phases.panels),
            ms(self.phases.render),
            ms(pipeline.timings.0),
            ms(pipeline.timings.1),
            ms(pipeline.timings.2),
            ms(pipeline.split.0),
            ms(pipeline.split.1),
            ms(pipeline.split.2),
        );
        self.phases = Phases::default();
        self.last_report = Instant::now();
        self.frames_at_report = self.frames;
        self.video_frames = 0;
    }

    /// How far the rect the border was drawn against is from where the
    /// picture really is, in pixels, worst case.
    ///
    /// This is the whole of the bug that darkens a border during a resize.
    /// `border_extend` clamps every one of its taps *into* the rect it is
    /// given, so a rect larger than the picture makes it average the black
    /// surround into the edge colour. It does not black the border out —
    /// which is why looking for black found nothing — it washes it darker,
    /// by an amount no threshold describes. The gap does.
    ///
    /// Measured rather than sampled: a readback would stall the pipeline
    /// every frame, and this needs no pixels.
    fn watch_rect(&mut self, player: &PlayerState, w: u32, h: u32) {
        let truth = player.fitted_rect(w, h)[1];
        let used = player.video_rect(w, h)[1];
        let gap = ((used - truth) * h as f32).abs();
        if gap > self.worst_gap + 0.5 {
            self.worst_gap = gap;
            eprintln!("dbm: border rect is {gap:.0}px from the picture at {w}x{h}");
        }
    }

    /// Read back the two places the effects show, and say which passes were
    /// meant to be running so the two can be compared. Returns the flags it
    /// sampled at.
    ///
    /// Two points because the effects do not overlap: the glass shows inside a
    /// panel, the ambient border only outside the video. Sampling one of them
    /// says nothing at all about the other.
    fn sample_enables(
        &self,
        pipeline: &Pipeline,
        gl: &glow::Context,
        player: &PlayerState,
    ) -> (bool, bool) {
        eprintln!(
            "dbm: glass={} ambience={}",
            pipeline.glass_enabled, pipeline.border_enabled
        );
        self.sample_panel(pipeline, gl);
        self.sample_rims(pipeline, gl, pipeline.glass_bevel());
        self.sample_letterbox(pipeline, gl, player);
        (pipeline.glass_enabled, pipeline.border_enabled)
    }

    /// Sample the letterbox, which is the only place the ambient border draws.
    /// With the pass off it is the background colour; with it on it carries
    /// the spread edge of the frame.
    fn sample_letterbox(&self, pipeline: &Pipeline, gl: &glow::Context, player: &PlayerState) {
        let (w, h) = pipeline.composite().size();
        let [x0, y0, _, y1] = player.video_rect(w, h);
        // Whichever margin exists; a clip that fills the window has neither,
        // and then there is nothing to say.
        let pt = if y0 > 0.02 {
            (w / 2, (y0 * h as f32 * 0.5) as u32)
        } else if x0 > 0.02 {
            ((x0 * w as f32 * 0.5) as u32, h / 2)
        } else {
            eprintln!("dbm: video fills the window, no letterbox to sample");
            return;
        };
        eprintln!(
            "dbm: letterbox at {pt:?}  output={:?} (mpv edge y={y0:.3}..{y1:.3}, \
             border drawn from {:?})",
            pipeline.output().sample_grid(gl, &[pt]),
            pipeline.border_rect(),
        );
    }

    /// Sample the middle of the bar, where the glass is. The composite is
    /// what the pass started from, so the pair shows the material working
    /// rather than just that something was drawn.
    fn sample_panel(&self, pipeline: &Pipeline, gl: &glow::Context) {
        let Some((rect, _)) = self.last_panels.last() else {
            eprintln!("dbm: no glass panel on screen to sample");
            return;
        };
        let pt = [(
            ((rect[0] + rect[2]) * 0.5) as u32,
            ((rect[1] + rect[3]) * 0.5) as u32,
        )];
        let (vw, vh) = pipeline.video().size();
        eprintln!(
            "dbm: panel centre  composite={:?} output={:?} | video centre={:?}",
            pipeline.composite().sample_grid(gl, &pt),
            pipeline.output().sample_grid(gl, &pt),
            pipeline.video().sample_grid(gl, &[(vw / 2, vh / 2)]),
        );
    }

    /// March inward from each panel's left edge and print what the glass
    /// does along the way.
    ///
    /// The bevel is the band where the surface is curved; past it the glass is
    /// flat and the pixels settle. So the distance at which a profile stops
    /// changing *is* the bevel width, and since the bevel is now a fraction of
    /// each panel's corner radius, a panel with twice the radius should take
    /// twice as far to settle. That is the claim, and it is not one a single
    /// sample at the centre could ever show.
    fn sample_rims(&self, pipeline: &Pipeline, gl: &glow::Context, bevel_ratio: f32) {
        for (rect, radius) in &self.last_panels {
            let half_h = 0.5 * (rect[3] - rect[1]);
            let half_w = 0.5 * (rect[2] - rect[0]);
            // The shader clamps the radius to what the box can hold.
            let radius = radius.min(half_w).min(half_h);
            let y = (0.5 * (rect[1] + rect[3])) as u32;
            let bevel = bevel_ratio * radius;

            // Sampled as fractions of this panel's own bevel, not in fixed
            // pixels. The dome puts nearly all its curvature in the last
            // stretch of the band, so fixed steps miss it entirely on a
            // narrow rim; and reading the same fractions on every panel is
            // what makes the profiles comparable at all. If the bevel really
            // does scale with the radius, these land at different pixel
            // offsets and yet trace the same curve.
            let fractions = [0.02f32, 0.06, 0.15, 0.3, 0.5, 0.8];
            let offsets: Vec<u32> = fractions
                .iter()
                .map(|f| ((bevel * f).round() as u32).max(1))
                .collect();
            let points: Vec<(u32, u32)> = offsets
                .iter()
                .map(|dx| ((rect[0] as u32).saturating_add(*dx), y))
                .collect();
            let greens: Vec<String> = pipeline
                .output()
                .sample_grid(gl, &points)
                .iter()
                .map(|px| format!("{:3}", px[1]))
                .collect();
            let at: Vec<String> = offsets.iter().map(|d| format!("{d:>2}")).collect();
            eprintln!(
                "dbm: rim r={radius:>4.0} bevel={bevel:>5.1}px at +[{}]px: {}",
                at.join(","),
                greens.join(" ")
            );
        }
    }

    /// One-shot dump of everything worth eyeballing when something looks
    /// wrong: the blur pyramid stage by stage, the parsed tracks, and the
    /// mirrored playback state.
    fn dump(&self, pipeline: &Pipeline, gl: &glow::Context, player: &PlayerState) {
        eprintln!("dbm: chain {}", pipeline.blur_chain_debug(gl));
        eprintln!(
            "dbm: state title={:?} t={:.2}/{:.2} ({:.0}%) paused={} vol={} rect={:?}",
            player.display_title(),
            player.time_pos,
            player.duration,
            player.progress() * 100.0,
            player.paused,
            player.volume,
            player.video_rect(
                pipeline.composite().size().0,
                pipeline.composite().size().1,
            ),
        );
        eprintln!("dbm: tracks ({} total)", player.tracks.len());
        // Per kind, so the dump shows what each menu section shows rather
        // than what a track would be called on its own.
        for kind in [TrackKind::Video, TrackKind::Audio, TrackKind::Sub] {
            for (t, label) in tracks::labelled(&player.tracks, kind) {
                eprintln!(
                    "       id={} {:?}{} {label:?}",
                    t.id,
                    t.kind,
                    if t.selected { " [selected]" } else { "" },
                );
            }
        }
        // As the panel labels it, heading and all, so the dump shows what is
        // on screen rather than a second opinion about it.
        let listing = crate::naming::listing(
            player
                .playlist
                .iter()
                .map(|e| (e.filename.as_str(), e.embedded_title())),
        );
        if let Some(show) = &listing.heading {
            eprintln!("  playlist heading {show:?}");
        }
        for (e, label) in player.playlist.iter().zip(&listing.rows) {
            eprintln!(
                "  playlist[{}]{} {label:?}",
                e.index,
                if e.current { " [current]" } else { "" },
            );
        }
        let subs = tracks::of_kind(&player.tracks, TrackKind::Sub).len();
        let audio = tracks::of_kind(&player.tracks, TrackKind::Audio).len();
        eprintln!("dbm: {subs} subtitle track(s), {audio} audio track(s)");
    }
}

impl Default for Probe {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// A picture of the frame, without a screen
// ---------------------------------------------------------------------------

/// One-shot capture of a finished frame, read straight off the framebuffer.
///
/// This exists because photographing the desktop is not the same as seeing
/// what the player drew. The desktop is a place where another window can be in
/// front, where a display can be asleep and nothing is presented at all, and
/// where a GPU-drawn surface comes back blank for reasons that have nothing to
/// do with this program. Every one of those happened while trying to look at
/// this interface. None of them is true of the framebuffer the frame was just
/// drawn into.
///
/// Taken in `AfterRendering`, so Slint's own scene — the text, the icons, the
/// highlights — is already composited over the pipeline's output and the
/// picture is the whole interface rather than the video half of it.
///
/// `DBM_CAPTURE=<path.bmp>`, and `DBM_CAPTURE_AFTER=<ms>` for how long to let
/// the interface settle first. BMP because it is a header and the pixels: a
/// PNG would mean an encoder, and anything that wants one can convert it.
pub struct Capture {
    path: Option<std::path::PathBuf>,
    after: std::time::Duration,
    started: Instant,
    done: bool,
}

impl Capture {
    pub fn new() -> Self {
        Self {
            path: std::env::var_os("DBM_CAPTURE").map(std::path::PathBuf::from),
            after: std::time::Duration::from_millis(
                std::env::var("DBM_CAPTURE_AFTER")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(4000),
            ),
            started: Instant::now(),
            done: false,
        }
    }

    pub fn maybe(&mut self, gl: &glow::Context, width: u32, height: u32) {
        if self.done || width == 0 || height == 0 {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.done = true;
            return;
        };
        if self.started.elapsed() < self.after {
            return;
        }
        self.done = true;

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        unsafe {
            use glow::HasContext;

            // The default framebuffer: what is about to be swapped to the
            // window, interface and all.
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        match write_bmp(&path, &pixels, width, height) {
            Ok(()) => eprintln!("dbm: captured {width}x{height} to {}", path.display()),
            Err(e) => eprintln!("dbm: capture failed: {e}"),
        }
    }
}

/// 32-bit bottom-up BMP.
///
/// `glReadPixels` hands back rows from the bottom up, which is the order a BMP
/// with a positive height already wants — so the rows go down as they are.
/// Only the channel order needs turning round: GL gives RGBA, BMP wants BGRA.
fn write_bmp(
    path: &std::path::Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> std::io::Result<()> {
    use std::io::Write;

    const FILE_HEADER: u32 = 14;
    const INFO_HEADER: u32 = 40;
    let data = width * height * 4;
    let mut out = Vec::with_capacity((FILE_HEADER + INFO_HEADER + data) as usize);

    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(FILE_HEADER + INFO_HEADER + data).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(FILE_HEADER + INFO_HEADER).to_le_bytes());

    out.extend_from_slice(&INFO_HEADER.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&data.to_le_bytes());
    // 72dpi, in the pixels-per-metre the format insists on.
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    std::fs::File::create(path)?.write_all(&out)
}
