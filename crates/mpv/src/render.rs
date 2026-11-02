//! mpv drawing into Slint's GL context, and the callbacks it makes while it
//! does.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::Arc;

use super::ffi::{
    GL_RGBA8, Lib, MpvRenderCtx, OpenglFbo, OpenglInitParams, RENDER_PARAM_API_TYPE,
    RENDER_PARAM_BLOCK_FOR_TARGET_TIME, RENDER_PARAM_FLIP_Y, RENDER_PARAM_INVALID,
    RENDER_PARAM_OPENGL_FBO, RENDER_PARAM_OPENGL_INIT_PARAMS, RenderParam, UPDATE_FRAME,
};
use super::{Mpv, Result, Wake, check, cstr};

/// Trampoline handing mpv's C-style `get_proc_address` to the Rust closure
/// Slint provides. `ctx` points at the fat pointer for that closure, which
/// stays alive for the duration of `mpv_render_context_create` - the only
/// window in which mpv calls this.
unsafe extern "C" fn get_proc_address_thunk(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: mpv names the function in a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) };
    if name == c"glFenceSync" {
        return no_fence as *mut c_void;
    }
    // SAFETY: `ctx` points at the closure reference `RenderContext::new`
    // passed, alive for the whole of the call this comes from.
    let f = unsafe { &*ctx.cast::<&dyn Fn(&CStr) -> *const c_void>() };
    f(name) as *mut c_void
}

/// What mpv is given for `glFenceSync`: no fence, ever.
///
/// Up to and including 0.41, mpv's GL renderer makes a fence for every frame
/// it draws and deletes them only in a buffer swap, which the render API never
/// does (fixed upstream in f74adc4, in no release yet). Each costs the driver
/// some kilobytes, NVIDIA's about five, so a film grew the player by its frame
/// rate times that, for as long as it played. A null fence is one mpv simply
/// doesn't keep. Its only other fences guard the upload buffers of direct
/// rendering, which libmpv uses only with `MPV_RENDER_PARAM_ADVANCED_CONTROL`,
/// never asked for here; asking for it would need real fences back.
unsafe extern "C" fn no_fence(_condition: u32, _flags: u32) -> *const c_void {
    std::ptr::null()
}

/// mpv's "a frame is ready" notification. Fires on an mpv-owned thread, so it
/// must not touch GL - it only pokes the Slint event loop, which comes back to
/// us on the UI thread in `BeforeRendering`.
unsafe extern "C" fn update_thunk(ctx: *mut c_void) {
    // SAFETY: `ctx` is the box `RenderContext::new` registered, freed only
    // after `drop` unregisters it.
    let notify = unsafe { &*ctx.cast::<Wake>() };
    notify();
}

/// mpv drawing into Slint's GL context. Every call into it, dropping it
/// included, needs that context current, as it is inside Slint's rendering
/// notifier.
pub struct RenderContext {
    /// Held so the handle outlives the render context, which mpv requires
    /// to be freed first.
    mpv: Arc<Mpv>,
    ctx: *mut MpvRenderCtx,
    /// Kept alive as long as mpv might invoke it, and freed in `Drop` only
    /// after the update callback has been unregistered.
    notify: *mut Wake,
}

impl RenderContext {
    /// Build the render context against the GL context Slint is already
    /// using. Must be called from a rendering-notifier callback, where that
    /// context is current and `get_proc_address` is live.
    ///
    /// # Safety
    /// The caller guarantees a current OpenGL context.
    pub unsafe fn new(
        mpv: Arc<Mpv>,
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        notify: Wake,
    ) -> Result<Self> {
        let api_type = cstr("opengl")?;
        let mut init = OpenglInitParams {
            get_proc_address: Some(get_proc_address_thunk),
            ctx: (&raw const get_proc_address).cast_mut().cast(),
        };
        let mut params = [
            RenderParam {
                kind: RENDER_PARAM_API_TYPE,
                data: api_type.as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&raw mut init).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];

        let mut ctx: *mut MpvRenderCtx = std::ptr::null_mut();
        // SAFETY: the caller has a GL context current, `handle` is live, and
        // `params` and all it points at outlive the call.
        check("mpv_render_context_create", unsafe {
            (mpv.lib.mpv_render_context_create)(&mut ctx, mpv.handle, params.as_mut_ptr())
        })?;
        drop(api_type);

        let notify = Box::into_raw(Box::new(notify));
        // SAFETY: `ctx` was just made, and `notify` lives until `drop`
        // unregisters it.
        unsafe {
            (mpv.lib.mpv_render_context_set_update_callback)(
                ctx,
                Some(update_thunk),
                notify.cast(),
            );
        };

        Ok(Self { mpv, ctx, notify })
    }

    fn lib(&self) -> &Lib {
        &self.mpv.lib
    }

    /// True when mpv has a new frame waiting. Cheap enough to poll every frame.
    ///
    /// # Safety
    /// Requires the same current GL context the render context was made on.
    pub unsafe fn wants_redraw(&self) -> bool {
        // SAFETY: the caller has the context current, and `ctx` is live until `drop`.
        unsafe { (self.lib().mpv_render_context_update)(self.ctx) & UPDATE_FRAME != 0 }
    }

    /// Draw the current frame into `fbo` at `w` x `h`.
    ///
    /// # Safety
    /// Requires the same current GL context the render context was made on.
    pub unsafe fn render_to_fbo(&self, fbo: u32, w: i32, h: i32) -> Result<()> {
        let mut target = OpenglFbo {
            fbo: fbo as c_int,
            w,
            h,
            internal_format: GL_RGBA8,
        };
        // 0 leaves row 0 of the texture holding the top of the image (checked
        // against a red-on-top clip), the top-down convention Slint's
        // `TopLeft` origin, the `osd-dimensions` margins and our passes
        // assume. 1 would flip it to GL's bottom-up order.
        let mut flip: c_int = 0;
        // Do not sleep until the frame is due.
        //
        // mpv's default blocks until the frame's display time, which on the
        // UI thread would mean a 33ms sleep per frame for a 30fps film, and
        // nothing could move faster than the film. `report_swap` keeps mpv's
        // timing informed instead.
        let mut block: c_int = 0;
        let mut params = [
            RenderParam {
                kind: RENDER_PARAM_OPENGL_FBO,
                data: (&raw mut target).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_FLIP_Y,
                data: (&raw mut flip).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                data: (&raw mut block).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: the caller has the context current, `ctx` is live, and
        // `params` and all it points at outlive the call.
        check("mpv_render_context_render", unsafe {
            (self.lib().mpv_render_context_render)(self.ctx, params.as_mut_ptr())
        })
    }

    /// Tell mpv the frame reached the screen, so its frame timing stays honest.
    ///
    /// # Safety
    /// Requires the same current GL context the render context was made on.
    pub unsafe fn report_swap(&self) {
        // SAFETY: the caller has the context current, and `ctx` is live until `drop`.
        unsafe { (self.lib().mpv_render_context_report_swap)(self.ctx) };
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        // SAFETY: it drops inside Slint's notifier, with the context current
        // (see the type's docs), and `notify` came from `Box::into_raw`.
        unsafe {
            // Unregister before freeing the closure, or a late callback from
            // mpv's thread would read freed memory.
            (self.lib().mpv_render_context_set_update_callback)(
                self.ctx,
                None,
                std::ptr::null_mut(),
            );
            (self.lib().mpv_render_context_free)(self.ctx);
            drop(Box::from_raw(self.notify));
        }
    }
}
