use crate::MainWindow;
use crate::app::App;
use crate::playback::commands;

/// The bar's own controls and what lies just beyond them: pause, mute,
/// panscan, the end of a file, the playlist, speed, frames and chapters.
pub(super) fn wire_transport(ui: &MainWindow, app: &App) {
    on!(ui, app, on_toggle_pause, |m| commands::toggle_pause(m));
    on!(ui, app, on_toggle_mute, |m| commands::toggle_mute(m));
    on!(ui, app, on_toggle_panscan, |m| commands::toggle_panscan(m));
    on!(ui, app, on_restart, |m| commands::restart(m));
    // What the end of a file offers: this one again, or the next one.
    on!(ui, app, on_replay, |m| commands::replay(m));
    // Every way of asking for another file on purpose goes through
    // `arrival`, which holds one with a place saved in it for the choice.
    // Not activity, here or for the pills' two answers below: each is the way
    // back to the film, and the file arriving shows its own title.
    on_app!(ui, app, on_play_next, |app| {
        app.arrival.step(&app.mpv, 1, Some(true));
    });
    on!(ui, app, on_nudge_volume, |m, d| commands::nudge_volume(
        m,
        f64::from(d)
    ));
    on!(ui, app, on_set_volume, |m, v| commands::set_volume(
        m,
        f64::from(v)
    ));
    // Not activity, for `on!`'s reason: Ctrl+arrows answer with a flash.
    on_app!(ui, app, on_playlist_step, |app, delta| {
        app.arrival.step(&app.mpv, delta, None);
    });
    on_app!(ui, app, on_play_index, |app, index| {
        app.activity.bump();
        app.arrival.play_index(&app.mpv, i64::from(index), None);
    });
    // The corner pill's offers: for a file autoplay started over, and back
    // from a click on the timeline.
    on_app!(ui, app, on_take_resume_offer, |app| {
        app.arrival.take_offer(&app.mpv);
    });
    on_app!(ui, app, on_take_back_offer, |app| {
        app.retrace.take(&app.mpv);
    });
    // The centre pill's answer, for a file held on its saved place.
    on_app!(ui, app, on_arrival_answer, |app, from_the_start| {
        app.arrival.answer(&app.mpv, from_the_start);
    });

    // Speed steps from the value the interface is showing, so the ladder is
    // decided here and mpv is only told where to land.
    on!(ui, app, on_step_speed, |m, current, direction| {
        commands::step_speed(m, f64::from(current), direction);
    });
    on!(ui, app, on_reset_speed, |m| commands::reset_speed(m));
    on!(ui, app, on_frame_step, |m, direction| commands::frame_step(
        m, direction
    ));
    on!(ui, app, on_chapter_step, |m, direction| {
        commands::chapter_step(m, direction);
    });

    ui.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}
