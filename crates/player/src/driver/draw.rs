use dbm_gpu::gfx::Target;
use dbm_gpu::pipeline::Frame;
use slint::{BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin, ComponentHandle};

use super::{Driver, Gpu, Published};
use crate::settings::Toggle;
use crate::{MainWindow, Playback};

impl Driver {
    /// Run the pipeline and hand Slint the result.
    pub(super) fn draw(&mut self, ui: &MainWindow, gpu: &mut Gpu, t: std::time::Instant) {
        // Material parameters, if a slider moved. Coalesced in the store,
        // so a drag costs one update per frame however fast it moves.
        let mut params_dirty = false;
        if let Some((glass, border)) = self.app.settings.take_changes() {
            gpu.pipeline.glass = glass;
            gpu.pipeline.params = border;
            gpu.pipeline.border_enabled = self.app.settings.is_on(Toggle::Ambience);
            gpu.pipeline.glass_enabled = self.app.settings.is_on(Toggle::Glass);
            ui.global::<crate::Paint>()
                .set_glass(self.app.settings.is_on(Toggle::Glass));
            gpu.pipeline.legible = self.app.settings.is_on(Toggle::LegibleGlass);
            params_dirty = true;
        }
        // What the interface moves by, the switch and the system together
        // (see `Motion.enabled`), which can change without the settings.
        gpu.pipeline.animate = ui.global::<crate::Motion>().get_enabled();

        // The backdrop stands in while nothing is loaded, and at the end of a
        // file once its light has come.
        let ended = self.player.eof_reached
            && self.per_file.end_lit.is_some()
            && self.per_file.end_lit == self.player.path;
        ui.global::<Playback>().set_end_lit(ended);
        gpu.pipeline.show_backdrop(
            self.player.path.is_none() || ended,
            self.player.path.is_some(),
        );
        let paint = ui.global::<crate::Paint>();
        gpu.pipeline.text_floor = f32::from(paint.get_body().alpha()) / 255.0;

        let size = ui.window().size();
        // For the size being rendered at, not the one mpv last reported on.
        let rect = self.player.video_rect(size.width, size.height);
        // Poll exactly once: reading the update flag also clears it, so a
        // second call would swallow the frame it reported. The context is
        // current, this being Slint's rendering notifier.
        let new_frame = unsafe { gpu.ctx.wants_redraw() };

        // None without the shadow, since nothing would give back what the
        // glass pass held.
        self.scratch.shaded.clear();
        if gpu.overlays.shadow.is_some() {
            dbm_gpu::shadow::regions(
                &self.scratch.panels,
                (size.width, size.height),
                ui.window().scale_factor(),
                &mut self.scratch.shaded,
            );
        }
        let frame = Frame {
            width: size.width,
            height: size.height,
            new_frame,
            params_dirty,
            rect,
            panels: &self.scratch.panels,
            shaded: &self.scratch.shaded,
        };
        // Called only from inside `render`, so still in the notifier, with
        // the context current.
        let film =
            |fbo, w, h| unsafe { gpu.ctx.render_to_fbo(fbo, w, h) }.map_err(|e| e.to_string());
        let rendered = match gpu.pipeline.render(&gpu.gl, film, &frame) {
            Ok(v) => v,
            Err(e) => {
                log::info!("pipeline: {e}");
                false
            }
        };
        self.diag.mark_render(t);
        if !rendered {
            return;
        }

        self.diag.frame_done(
            new_frame,
            &gpu.pipeline,
            &gpu.gl,
            &self.player,
            size.width,
            size.height,
        );

        publish(gpu.pipeline.output(), &mut gpu.published, |img, w, h| {
            ui.set_video_frame(img);
            ui.set_video_clip_w(w);
            ui.set_video_clip_h(h);
        });
    }
}
/// Point a Slint image property at a target, but only when something actually
/// changed — `Property::set` skips equal values, so a no-op costs nothing but
/// also invalidates nothing.
///
/// The `Image` covers the whole texture, which is generally larger than the
/// active image because targets grow only. `set` receives the active size so
/// the UI can clip the source rect down to it.
fn publish(
    target: &Target,
    slot: &mut Option<Published>,
    set: impl FnOnce(slint::Image, i32, i32),
) {
    let Some(id) = target.texture_id() else {
        return;
    };
    let (uw, uh) = target.size();
    let (aw, ah) = target.alloc();
    let key = (id, uw, uh, aw, ah);
    if *slot == Some(key) {
        return;
    }
    let image =
        unsafe { BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(id, (aw, ah).into()) }
            // mpv renders unflipped and every pass preserves that, so the
            // whole chain reads top-left like the rest of the app.
            .origin(BorrowedOpenGLTextureOrigin::TopLeft)
            .build();
    set(image, uw as i32, uh as i32);
    *slot = Some(key);
}
