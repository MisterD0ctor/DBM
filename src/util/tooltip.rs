//! Keep every tooltip floating inside the viewport with an 8px margin.
//!
//! Each tooltip in the app is absolutely-positioned and centered on its
//! anchor with a `translateX(-50%)` trick. When the anchor sits near the
//! window edge — a toolbar button against the right side, the seek cursor
//! near the start of the track, the volume thumb at 0%/100% — the tooltip
//! overflows the window.
//!
//! Solution: every tooltip transform includes a `var(--clamp-x, 0px)` term
//! (see styles.css). On pointer activity (and on resize) we measure each
//! tooltip's natural bounding rect, compute the horizontal nudge needed to
//! bring it inside `[MARGIN, viewport_w - MARGIN]`, and write that nudge
//! into the element's `--clamp-x`. The CSS does the rest.
//!
//! One `requestAnimationFrame` per pointer event coalesces multiple events
//! into a single layout/measure pass.

use std::cell::{Cell, RefCell};

use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

const MARGIN: f64 = 8.0;
const SELECTORS: &str = ".tooltip, .seek-tooltip";

thread_local! {
    static PENDING: Cell<bool> = const { Cell::new(false) };
    static RAF_CB: RefCell<Option<Closure<dyn Fn()>>> = const { RefCell::new(None) };
}

pub fn install_tooltip_clamp() {
    // Reasons to re-measure: pointer entered a new anchor (pointerover),
    // pointer moved (seek cursor, volume thumb, slider scrub), or the
    // viewport size changed.
    window_event_listener(ev::pointermove, |_| schedule());
    window_event_listener(ev::pointerover, |_| schedule());
    window_event_listener(ev::resize, |_| schedule());
    // First pass before any pointer activity — covers tooltips that are
    // always live (e.g. the seek tooltip during a scrub-on-load).
    schedule();
}

fn schedule() {
    if PENDING.with(|p| p.replace(true)) {
        return;
    }
    let Some(win) = web_sys::window() else {
        PENDING.with(|p| p.set(false));
        return;
    };
    RAF_CB.with(|cb_cell| {
        let mut borrow = cb_cell.borrow_mut();
        if borrow.is_none() {
            *borrow = Some(Closure::<dyn Fn()>::new(|| {
                PENDING.with(|p| p.set(false));
                clamp_all();
            }));
        }
        if let Some(cb) = borrow.as_ref() {
            let f: &js_sys::Function = cb.as_ref().unchecked_ref();
            let _ = win.request_animation_frame(f);
        }
    });
}

fn clamp_all() {
    let Some(win) = web_sys::window() else { return };
    let vw = win
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if vw <= 0.0 {
        return;
    }
    let Some(doc) = win.document() else { return };
    let Ok(list) = doc.query_selector_all(SELECTORS) else { return };
    for i in 0..list.length() {
        let Some(node) = list.item(i) else { continue };
        let Ok(el) = node.dyn_into::<web_sys::HtmlElement>() else { continue };
        clamp_one(&el, vw);
    }
}

fn clamp_one(el: &web_sys::HtmlElement, vw: f64) {
    let style = el.style();
    // Reset before measuring so we read the natural (un-nudged) position.
    let _ = style.set_property("--clamp-x", "0px");
    let rect = el.get_bounding_client_rect();
    // Skip elements that aren't rendered (display:none ancestors → zero rect).
    if rect.width() <= 0.0 {
        return;
    }
    let delta = if rect.left() < MARGIN {
        MARGIN - rect.left()
    } else if rect.right() > vw - MARGIN {
        (vw - MARGIN) - rect.right()
    } else {
        0.0
    };
    if delta.abs() > 0.5 {
        let _ = style.set_property("--clamp-x", &format!("{:.2}px", delta));
    }
}
