//! The settings pages' slider rows.

use std::rc::Rc;

use dbm_lang::tr;
use slint::ComponentHandle;
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::interface::format;
use crate::settings::{self, Section, Store};
use crate::{MainWindow, ParamItem, Settings};

/// The slider rows, split by the page they appear on.
///
/// Two models rather than one filtered in the UI: each page binds to its own
/// list, and a row carries its registry index so the callback needs no
/// arithmetic to map back.
pub struct ParamModels {
    glass: Rc<VecModel<ParamItem>>,
    border: Rc<VecModel<ParamItem>>,
    picture: Rc<VecModel<ParamItem>>,
}

impl ParamModels {
    pub fn build(ui: &MainWindow, store: &Store) -> Self {
        let settings = ui.global::<Settings>();
        let models = Self {
            glass: section_model(store, Section::Glass),
            border: section_model(store, Section::Border),
            picture: section_model(store, Section::Picture),
        };
        settings.set_glass_params(ModelRc::from(models.glass.clone()));
        settings.set_border_params(ModelRc::from(models.border.clone()));
        settings.set_picture_params(ModelRc::from(models.picture.clone()));
        settings.set_param_names(names());
        models
    }

    /// Every page again, in the language now spoken.
    pub fn say_again(&self, ui: &MainWindow, store: &Store) {
        self.refresh_all(store);
        ui.global::<Settings>().set_param_names(names());
    }

    /// Refresh one row in place.
    ///
    /// Not a rebuild, because replacing a model recreates every row,
    /// destroying the `TouchArea` the pointer is holding mid-drag.
    pub fn update(&self, store: &Store, index: usize) {
        let Some(param) = settings::REGISTRY.get(index) else {
            return;
        };
        let model = self.model_for(param.section);
        if let Some(row) = rows_of(param.section).position(|i| i == index) {
            model.set_row_data(row, param_row(store, index));
        }
    }

    /// Rebuild every page. For a reset, where every row moved at once and no
    /// drag is in progress.
    pub fn refresh_all(&self, store: &Store) {
        for (section, model) in [
            (Section::Glass, &self.glass),
            (Section::Border, &self.border),
            (Section::Picture, &self.picture),
        ] {
            for (row, index) in rows_of(section).enumerate() {
                model.set_row_data(row, param_row(store, index));
            }
        }
    }

    fn model_for(&self, section: Section) -> &VecModel<ParamItem> {
        match section {
            Section::Glass => &self.glass,
            Section::Border => &self.border,
            Section::Picture => &self.picture,
        }
    }
}

/// Every slider's name, one to a line, for the interface to measure: every
/// page sets its names in one column, as wide as the widest of them in the
/// language spoken.
fn names() -> SharedString {
    let names: Vec<&str> = settings::REGISTRY.iter().map(|p| tr(p.label)).collect();
    names.join("\n").into()
}

fn rows_of(section: Section) -> impl Iterator<Item = usize> {
    (0..settings::REGISTRY.len()).filter(move |i| settings::REGISTRY[*i].section == section)
}

fn section_model(store: &Store, section: Section) -> Rc<VecModel<ParamItem>> {
    Rc::new(VecModel::from(
        rows_of(section)
            .map(|i| param_row(store, i))
            .collect::<Vec<_>>(),
    ))
}

fn param_row(store: &Store, index: usize) -> ParamItem {
    let param = &settings::REGISTRY[index];
    let value = store.value(param.key);
    let span = (param.max - param.min).max(f32::EPSILON);
    ParamItem {
        index: index as i32,
        label: tr(param.label).into(),
        hint: if param.hint.is_empty() {
            Default::default()
        } else {
            tr(param.hint).into()
        },
        fraction: ((value - param.min) / span).clamp(0.0, 1.0),
        // Where none of it is: the start, or for a value either side of
        // nothing, the middle.
        origin: ((0.0 - param.min) / span).clamp(0.0, 1.0),
        readout: format::readout(value, param.min, param.max).into(),
    }
}
