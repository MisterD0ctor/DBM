use dbm_gpu::gfx::Target;
use dbm_gpu::pipeline::{Backdrop, Frame};
use dbm_gpu::size::Size;
use slint::{BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin, ComponentHandle};

use super::{Driver, Gpu, Published};
use crate::MainWindow;
use crate::player::Player;
use crate::settings::Toggle;

impl Driver {
    /// Run the pipeline and hand Slint the result.
    pub(super) fn draw(
        &mut self,
        ui: &MainWindow,
        gpu: &mut Gpu,
        player: &Player,
        t: std::time::Instant,
    ) {
        let state = player.state();
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
        gpu.pipeline
            .show_backdrop(match (state.path.is_some(), player.end_lit()) {
                (false, _) => Backdrop::Alone,
                (true, true) => Backdrop::OverFilm,
                (true, false) => Backdrop::Off,
            });
        let paint = ui.global::<crate::Paint>();
        gpu.pipeline.text_floor = f32::from(paint.get_body().alpha()) / 255.0;

        let size = ui.window().size();
        // For the size being rendered at, not the one mpv last reported on.
        let rect = state.video_rect(size.width, size.height);
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
                Size::new(size.width, size.height),
                ui.window().scale_factor(),
                &mut self.scratch.shaded,
            );
        }
        let frame = Frame {
            size: Size::new(size.width, size.height),
            new_frame,
            params_dirty,
            rect,
            panels: &self.scratch.panels,
            shaded: &self.scratch.shaded,
        };
        // Called only from inside `render`, so still in the notifier, with
        // the context current.
        let film = |fbo, w, h| {
            unsafe { gpu.ctx.render_to_fbo(fbo, w, h) }.map_err(dbm_gpu::pipeline::Error::film)
        };
        let rendered = match gpu.pipeline.render(&gpu.gl, film, &frame) {
            Ok(v) => v,
            Err(e) => {
                log::info!("pipeline: {e}");
                false
            }
        };
        self.diag.mark_render(t);
        if !rendered {
            // A failed grow may have replaced the output target, and a
            // failed film render may have left the replacement cleared.
            // Either way, what Slint holds must not be a texture name that
            // has already been deleted.
            if gpu.pipeline.output().texture_id().is_some() {
                publish(gpu.pipeline.output(), &mut gpu.published, |img, w, h| {
                    show(ui, img, w, h);
                });
            } else {
                gpu.published = None;
                clear_image(ui);
            }
            return;
        }

        self.diag.frame_done(
            new_frame,
            &gpu.pipeline,
            &gpu.gl,
            state,
            size.width,
            size.height,
        );

        publish(gpu.pipeline.output(), &mut gpu.published, |img, w, h| {
            show(ui, img, w, h);
        });
    }
}

/// Put one image on the interface, clipped to the active part of its target.
fn show(ui: &MainWindow, image: slint::Image, w: i32, h: i32) {
    ui.set_video_frame(image);
    ui.set_video_clip_w(w);
    ui.set_video_clip_h(h);
}

/// Stop showing any image, so no deleted texture name outlives its target.
pub(super) fn clear_image(ui: &MainWindow) {
    ui.set_video_frame(slint::Image::default());
    ui.set_video_clip_w(0);
    ui.set_video_clip_h(0);
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
    let used = target.size();
    let alloc = target.alloc();
    let key = Published { id, used, alloc };
    if *slot == Some(key) {
        return;
    }
    let image = unsafe {
        BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(id, (alloc.width, alloc.height).into())
    }
    // mpv renders unflipped and every pass preserves that, so the
    // whole chain reads top-left like the rest of the app.
    .origin(BorrowedOpenGLTextureOrigin::TopLeft)
    .build();
    set(image, used.width as i32, used.height as i32);
    *slot = Some(key);
}
