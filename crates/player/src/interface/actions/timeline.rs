use slint::ComponentHandle;

use crate::app::App;
use crate::playback::commands;
use crate::{MainWindow, Playback};

/// Seeking, scrubbing, and the preview over the timeline.
pub(super) fn wire_timeline(ui: &MainWindow, app: &App) {
    let playback = ui.global::<Playback>();
    on!(playback, app, on_seek_relative, |m, secs| {
        commands::seek_relative(m, f64::from(secs));
    });
    // A number key's jump: anywhere in the film from one press, which loses a
    // place as easily as a click on the timeline, so the way back is offered
    // the same way. Not activity, for `on!`'s reason: the timeline answers.
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playback.on_jump_to(move |fraction| {
            let Some(ui) = weak.upgrade() else { return };
            let duration = f64::from(ui.global::<Playback>().get_duration());
            let to = f64::from(fraction) * duration;
            if let Some(path) = super::playing(&ui) {
                let at = f64::from(ui.global::<Playback>().get_progress()) * duration;
                app.retrace.landed(&path, at, to);
            }
            commands::seek_to(&app.mpv, to);
        });
    }
    // Past the title sequence, to where it was found to end, or past the
    // credits, to the scene after them.
    on!(playback, app, on_skip_to, |m, to| commands::seek_to(
        m,
        f64::from(to)
    ));
    // Release: land exactly, and drop any coalesced drag update, which is
    // now stale by definition.
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playback.on_seek_fraction(move |f| {
            app.activity.bump();
            if let Some(ui) = weak.upgrade()
                && let Some(path) = super::playing(&ui)
            {
                let duration = f64::from(ui.global::<Playback>().get_duration());
                let at = f64::from(ui.global::<Playback>().get_progress()) * duration;
                app.retrace.landed(&path, at, f64::from(f) * duration);
            }
            app.scrubber.release(&app.mpv, f);
        });
    }
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playback.on_seek_scrub(move |f| {
            app.activity.bump();
            // From the mirrored state, the cheapest source: whether it is
            // paused, which matters on a drag's first update, the film's
            // length, which turns distance along the track into speed, and
            // where it was, which is where a drag's first update leaves.
            let (paused, duration, progress) = weak.upgrade().map_or((false, 0.0, 0.0), |ui| {
                (
                    ui.global::<Playback>().get_paused(),
                    ui.global::<Playback>().get_duration(),
                    ui.global::<Playback>().get_progress(),
                )
            });
            app.retrace.hold(f64::from(progress) * f64::from(duration));
            app.scrubber
                .drag_to(&app.mpv, f, paused, f64::from(duration));
        });
    }
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playback.on_seek_held(move |f| {
            app.activity.bump();
            if let Some(ui) = weak.upgrade() {
                app.retrace.hold(
                    f64::from(ui.global::<Playback>().get_progress())
                        * f64::from(ui.global::<Playback>().get_duration()),
                );
            }
            app.scrubber.held_at(&app.mpv, f);
        });
    }
    // Answered from what is already known (the duration and the atlas),
    // because this fires on every pointer move over the timeline, where
    // nothing may touch mpv or the disk.
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playback.on_preview_at(move |fraction| {
            let Some(ui) = weak.upgrade() else { return };
            let answer = app.preview.answer(fraction);
            ui.global::<Playback>()
                .set_preview_time(crate::interface::format::time(answer.seconds).into());
            ui.global::<Playback>()
                .set_preview_chapter(answer.chapter.into());
            ui.global::<Playback>().set_preview_clip_x(answer.x);
            ui.global::<Playback>().set_preview_clip_y(answer.y);
        });
    }
}
