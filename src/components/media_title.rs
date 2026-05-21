//! Renders the current file as either a TV-show breakdown (show / episode /
//! title) or a plain cleaned filename. Replaces `setMediaTitle` from the
//! old JS app — same shape, but no DOM diffing by hand.

use std::time::Duration;

use leptos::html;
use leptos::prelude::*;

use crate::components::toolbar::{Reflow, Row, Side};
use crate::state::PlayerState;
use crate::util::parse::{clean_separators, parse_tv_show, strip_extension, strip_metadata};

#[component]
pub fn MediaTitle(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();
    let hidden = move || reflow.hidden(row, Side::Start, "media-title");
    let el_ref = NodeRef::<html::Div>::new();

    // Only the Main-row copy publishes its measured width — its margins differ
    // from the Overflow-row copy's, and the reflow math is anchored to the
    // main toolbar's geometry. Main is hidden once media-title overflows
    // (level ≥ 1), at which point `scrollWidth` is 0 and we keep the previous
    // value; the next time Main reappears (window grows / filename changes)
    // we re-measure.
    let measure_self = matches!(row, Row::Main);
    Effect::new(move |_| {
        let _ = state.filename.get();
        let _ = hidden();
        if !measure_self {
            return;
        }
        set_timeout(
            move || {
                let Some(el) = el_ref.get_untracked() else {
                    return;
                };
                let scroll_w = el.scroll_width() as f64;
                if scroll_w <= 0.0 {
                    return;
                }
                // scrollWidth ignores the element's own margin, but flex
                // layout counts it — read it back so a CSS tweak to the
                // media-title margin doesn't desync the reflow math.
                let (ml, mr) = horizontal_margins(&el);
                let w = scroll_w + ml + mr;
                let current = reflow.media_title_width.get_untracked();
                if (w - current).abs() > 0.5 {
                    reflow.media_title_width.set(w);
                }
            },
            Duration::from_millis(0),
        );
    });

    view! {
        <div class="media-title" class:hidden=hidden node_ref=el_ref>
            {move || render(state.filename.get())}
        </div>
    }
}

fn horizontal_margins(el: &web_sys::HtmlElement) -> (f64, f64) {
    let Some(win) = web_sys::window() else {
        return (0.0, 0.0);
    };
    let Ok(Some(cs)) = win.get_computed_style(el) else {
        return (0.0, 0.0);
    };
    let parse = |p: &str| -> f64 {
        cs.get_property_value(p)
            .ok()
            .and_then(|s| s.trim_end_matches("px").trim().parse().ok())
            .unwrap_or(0.0)
    };
    (parse("margin-left"), parse("margin-right"))
}

fn render(filename: Option<String>) -> AnyView {
    let Some(raw) = filename else {
        return ().into_any();
    };
    let name = strip_extension(&raw);
    if let Some(tv) = parse_tv_show(name) {
        let ep = format!("S{:02}:E{:02}", tv.season, tv.episode);
        view! {
            <span class="show">{tv.show}</span>
            <span class="episode">{ep}</span>
            {tv.title.map(|t| view! { <span class="episode-title">{t}</span> })}
        }
        .into_any()
    } else {
        let plain = strip_metadata(&clean_separators(name));
        view! { <span class="title">{plain}</span> }.into_any()
    }
}
