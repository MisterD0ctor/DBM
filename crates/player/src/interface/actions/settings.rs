use std::rc::Rc;

use slint::ComponentHandle;

use super::Undo;
use crate::app::App;
use crate::interface::sliders;
use crate::playback::commands;
use crate::settings::{REGISTRY, Section, Toggle};
use crate::{MainWindow, Motion, Reset, Settings, Tip};

/// The glass and ambient border pages: sliders, resets and their undo, and
/// the two switches beside them.
pub(super) fn wire_settings(ui: &MainWindow, app: &App, undo: &Undo) {
    let settings = ui.global::<Settings>();
    // Sliders report a normalised position; the registry owns the range.
    // The model is updated here so the readout follows the drag.
    let models = Rc::new(sliders::ParamModels::build(ui, &app.settings));
    {
        let (app, models) = (app.clone(), models.clone());
        ui.global::<Settings>()
            .on_set_param(move |index, fraction| {
                app.activity.bump();
                let index = index as usize;
                let Some(param) = REGISTRY.get(index) else {
                    return;
                };
                app.settings.set(
                    index,
                    param.min + fraction.clamp(0.0, 1.0) * (param.max - param.min),
                );
                if param.section == Section::Picture {
                    commands::set_picture(&app.mpv, &app.settings.picture());
                }
                models.update(&app.settings, index);
            });
    }
    {
        let (app, models, undo) = (app.clone(), models.clone(), undo.clone());
        settings.on_reset_section(move |what| {
            app.activity.bump();
            if let Some(section) = section_of(what) {
                undo.set(Some(app.settings.before()));
                app.settings.reset(section);
                if section == Section::Picture {
                    commands::set_picture(&app.mpv, &app.settings.picture());
                }
                // Every row moved and no drag can be in progress, so a full
                // refresh is both safe and simplest.
                models.refresh_all(&app.settings);
            }
        });
    }
    // The subtitles' timing is put back by their menu itself, since it was
    // never a setting.
    {
        let (app, models, undo) = (app.clone(), models.clone(), undo.clone());
        settings.on_undo_reset(move |what| {
            app.activity.bump();
            let Some(was) = undo.take() else {
                return;
            };
            match what {
                Reset::Glass => app.settings.restore(Section::Glass, &was),
                Reset::Border => app.settings.restore(Section::Border, &was),
                Reset::Picture => {
                    app.settings.restore(Section::Picture, &was);
                    commands::set_picture(&app.mpv, &app.settings.picture());
                }
                Reset::Placing => {
                    app.settings
                        .set_sub_scale(
                            commands::set_sub_scale(&app.mpv, f64::from(was.sub_scale)) as f32
                        );
                    app.subline.set(&app.mpv, f64::from(was.sub_pos));
                }
                Reset::None | Reset::Timing => {}
            }
            models.refresh_all(&app.settings);
        });
    }

    // Effect toggles, separate from the sliders rather than a zero on one,
    // so the tuning survives switching an effect off.
    // Not activity, for `on!`'s reason: B is answered by the border.
    on_app!(settings, app, on_set_ambience, |app, on| {
        app.settings.turn(Toggle::Ambience, on);
    });
    on_app!(settings, app, on_set_autoplay, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::Autoplay, on);
        commands::set_autoplay(&app.mpv, on);
    });
    // Read by the frame loop as it plays (see `skips`), so saving is all.
    on_app!(settings, app, on_set_skip_intros, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::SkipIntros, on);
    });
    on_app!(settings, app, on_set_skip_credits, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::SkipCredits, on);
    });
    // `Motion.enabled` is already set on the Slint side, so this saves the
    // choice, which the driver also hands the pipeline for the backdrop's
    // fade.
    on_app!(settings, app, on_set_animations, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::Animations, on);
    });
    // The right end of the timeline, the whole length or what is left, from
    // a click on it or the video page's switch.
    on_app!(settings, app, on_set_show_remaining, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::Remaining, on);
    });
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        settings.on_step_scale(move |direction| {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            // From the size shown, which is under the one saved on a display
            // without room for that, so a step down is always one.
            let wanted = ui.global::<Settings>().get_scale()
                + direction as f32 * crate::settings::SCALE_STEP;
            let drawn = app.scale.apply(&ui, crate::settings::offered_scale(wanted));
            app.settings.set_scale(drawn);
        });
    }
    // Saved, and handed the pipeline by the driver with the material.
    on_app!(settings, app, on_set_legible_glass, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::LegibleGlass, on);
    });
    // The tooltips' two switches, likewise already acted on by `Tip`.
    on_app!(settings, app, on_set_tips, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::Tips, on);
    });
    on_app!(settings, app, on_set_tip_keys, |app, on| {
        app.activity.bump();
        app.settings.turn(Toggle::TipKeys, on);
    });

    // The rows of the language page, as it numbers them: the system's, then
    // English, then each language built in. The row is already lit by the
    // time this runs; what is left is speaking it and saving it.
    {
        let (app, models, weak) = (app.clone(), models.clone(), ui.as_weak());
        settings.on_choose_language(move |row| {
            app.activity.bump();
            let code = language_of(row);
            app.settings.set_language(&code);
            speak(&code);
            if let Some(ui) = weak.upgrade() {
                models.say_again(&ui, &app.settings);
            }
        });
    }
}

/// The section of the settings a reset row puts back, where it is one.
fn section_of(what: Reset) -> Option<Section> {
    match what {
        Reset::Glass => Some(Section::Glass),
        Reset::Border => Some(Section::Border),
        Reset::Picture => Some(Section::Picture),
        Reset::None | Reset::Timing | Reset::Placing => None,
    }
}

/// The language a row of the language page stands for, by code: nothing for
/// the system's, which is the first row, and English for the second.
pub(super) fn language_of(row: i32) -> String {
    match row {
        0 => String::new(),
        1 => "en".into(),
        n => dbm_lang::codes()
            .nth((n - 2).max(0) as usize)
            .unwrap_or_default()
            .to_string(),
    }
}

/// The row that stands for a saved choice. A language this build no longer
/// has is the system's row, which is also how it is spoken.
pub(super) fn row_of(code: &str) -> i32 {
    match code {
        "" => 0,
        "en" => 1,
        code => dbm_lang::codes()
            .position(|known| known == code)
            .map_or(0, |place| place as i32 + 2),
    }
}

/// Put what was saved where it acts: the switches on the settings page, and
/// the preferences mpv has to be told about.
pub(super) fn restore_preferences(ui: &MainWindow, app: &App) {
    let settings = &app.settings;
    ui.global::<Settings>()
        .set_ambience_on(settings.is_on(Toggle::Ambience));
    ui.global::<Settings>()
        .set_autoplay(settings.is_on(Toggle::Autoplay));
    ui.global::<Settings>()
        .set_skip_intros(settings.is_on(Toggle::SkipIntros));
    ui.global::<Settings>()
        .set_skip_credits(settings.is_on(Toggle::SkipCredits));
    ui.global::<Motion>()
        .set_chosen(settings.is_on(Toggle::Animations));
    ui.global::<Settings>()
        .set_legible_glass(settings.is_on(Toggle::LegibleGlass));
    let tip = ui.global::<Tip>();
    tip.set_enabled(settings.is_on(Toggle::Tips));
    tip.set_with_keys(settings.is_on(Toggle::TipKeys));
    commands::set_autoplay(&app.mpv, settings.is_on(Toggle::Autoplay));
    commands::set_sub_scale(&app.mpv, f64::from(settings.sub_scale()));
    app.subline.set(&app.mpv, f64::from(settings.sub_pos()));
    commands::set_picture(&app.mpv, &settings.picture());
    ui.global::<Settings>()
        .set_show_remaining(settings.is_on(Toggle::Remaining));
    // Given the window on the first frame (see `Scale::frame`), once the
    // system has given it its own.
    ui.global::<Settings>().set_scale(settings.scale());
}

/// Speak the language that was saved, and list the ones it can be changed to.
pub(super) fn restore_language(ui: &MainWindow, app: &App) {
    // Each language by its own name for itself, which is the one name for it
    // that its speakers can read whatever the interface is in.
    ui.global::<Settings>()
        .set_language_names(slint::ModelRc::new(slint::VecModel::from(
            dbm_lang::codes()
                .map(|code| dbm_naming::language_name(code).into())
                .collect::<Vec<slint::SharedString>>(),
        )));
    let language = app.settings.language();
    ui.global::<Settings>()
        .set_language_choice(row_of(&language));
    speak(&language);
}

/// Speak a language (see `dbm_lang::speak`), the interface's own words too.
///
/// **UI thread, after the window exists** — Slint's half of the switch needs
/// a component to have been created.
fn speak(choice: &str) {
    let spoken = dbm_lang::speak(choice);
    if let Err(e) = slint::select_bundled_translation(spoken.unwrap_or_default()) {
        log::warn!("could not switch the interface to {spoken:?}: {e}");
    }
}
