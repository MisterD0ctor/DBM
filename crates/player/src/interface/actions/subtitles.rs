use slint::ComponentHandle;

use super::Undo;
use crate::app::App;
use crate::playback::commands;
use crate::worker::Completion;
use crate::{MainWindow, Subtitles};

/// The subtitles page: timing, size and placement, and lining the timing up
/// with the film's speech.
///
/// Size and placement are nudged from the stored value rather than the
/// mirrored one, because the store is what gets saved, and one source keeps
/// them from drifting apart.
pub(super) fn wire_subtitles(ui: &MainWindow, app: &App, undo: &Undo) {
    let subtitles = ui.global::<Subtitles>();
    on!(subtitles, app, on_nudge_delay, |m, d| {
        commands::nudge_sub_delay(m, f64::from(d));
    });
    on_app!(subtitles, app, on_nudge_scale, |app, delta| {
        app.activity.bump();
        let sent = commands::set_sub_scale(
            &app.mpv,
            f64::from(app.settings.sub_scale()) + f64::from(delta),
        );
        app.settings.set_sub_scale(sent as f32);
    });
    on_app!(subtitles, app, on_nudge_pos, |app, delta| {
        app.activity.bump();
        app.subline.set(
            &app.mpv,
            f64::from(app.settings.sub_pos()) + f64::from(delta),
        );
    });
    {
        let (app, undo) = (app.clone(), undo.clone());
        subtitles.on_reset(move || {
            app.activity.bump();
            undo.set(Some(app.settings.before()));
            app.settings
                .set_sub_scale(
                    commands::set_sub_scale(&app.mpv, commands::SUB_SCALE_DEFAULT) as f32,
                );
            app.subline.set(&app.mpv, commands::SUB_POS_DEFAULT);
        });
    }
    // Listening takes seconds and runs on a thread of its own, started by the
    // worker, whose reads of mpv it begins with; the answer comes back as a
    // completion, which is where the timing is applied.
    on_app!(subtitles, app, on_sync, |app| {
        app.activity.bump();
        let reporter = app.worker.reporter(Completion::Synced);
        app.worker
            .run(move |mpv| dbm_library::subsync::spawn(mpv.clone(), reporter));
    });
    on_app!(subtitles, app, on_set_timing, |app, delay, speed| {
        app.activity.bump();
        commands::set_sub_delay(&app.mpv, f64::from(delay));
        commands::set_sub_speed(&app.mpv, f64::from(speed));
        // Saved at once, as a sync is.
        crate::playback::session::checkpoint(&app.mpv);
    });
}
