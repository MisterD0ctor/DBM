//! The settings pages' slider rows.

use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};

use crate::interface::format;
use crate::settings::{self, Section, Store};
use crate::{MainWindow, ParamItem};

/// The slider rows, split by the page they appear on.
///
/// Two models rather than one filtered in the UI: each page binds to its own
/// list, and a row carries its registry index so the callback needs no
/// arithmetic to map back.
pub struct ParamModels {
    glass: Rc<VecModel<ParamItem>>,
    border: Rc<VecModel<ParamItem>>,
}

impl ParamModels {
    pub fn build(ui: &MainWindow, store: &Store) -> Self {
        let models = Self {
            glass: section_model(store, Section::Glass),
            border: section_model(store, Section::Border),
        };
        ui.set_glass_params(ModelRc::from(models.glass.clone()));
        ui.set_border_params(ModelRc::from(models.border.clone()));
        models
    }

    /// Refresh one row in place.
    ///
    /// Deliberately not a rebuild: replacing a model makes the repeater tear
    /// down and recreate every row, destroying the `TouchArea` the pointer is
    /// holding — which is what once made the sliders click-only.
    pub fn update(&self, store: &Store, index: usize) {
        let Some(param) = settings::REGISTRY.get(index) else {
            return;
        };
        let model = self.model_for(param.section);
        if let Some(row) = rows_of(param.section).position(|i| i == index) {
            model.set_row_data(row, param_row(store, index));
        }
    }

    /// Rebuild both pages. For a reset, where every row moved at once and no
    /// drag is in progress.
    pub fn refresh_all(&self, store: &Store) {
        for (section, model) in [
            (Section::Glass, &self.glass),
            (Section::Border, &self.border),
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
        }
    }
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
    let value = store.value(index);
    let span = (param.max - param.min).max(f32::EPSILON);
    ParamItem {
        index: index as i32,
        label: param.label.into(),
        fraction: ((value - param.min) / span).clamp(0.0, 1.0),
        readout: format::readout(value, param.min, param.max).into(),
    }
}
