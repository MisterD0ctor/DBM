//! The search for subtitles on OpenSubtitles, as the subtitles menu shows it:
//! the rows a search found, the download a sign-in is waited on for, and
//! what each answer does to the page.
//!
//! The callbacks start the work (see `actions`), and the answers arrive here
//! through the driver's completions. Every answer is for the film it was
//! asked about, so one for a film since replaced fills nothing.

use std::cell::RefCell;

use dbm_lang::tr;
use dbm_library::opensubtitles::account::Account;
use dbm_library::opensubtitles::{self, Failure, Fetched, Found, Hit};
use dbm_library::threads::ForFile;
use dbm_mpv::{MediaPath, Mpv};
use dbm_naming::Media;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::playback::commands;
use crate::worker::{Completion, Worker};
use crate::{Find, FindState, FoundItem, MainWindow};

#[derive(Default)]
pub struct Search(RefCell<State>);

#[derive(Default)]
struct State {
    /// What the last search found, in the order its rows are drawn.
    found: Vec<Hit>,
    /// The result whose download was refused for want of an account, kept
    /// with the film it was for: fetched again once the person signs in,
    /// which is what they signed in for.
    waiting: Option<ForFile<Hit>>,
}

impl Search {
    /// Forget the last search's rows, as a new one starts.
    pub fn clear(&self) {
        self.0.borrow_mut().found.clear();
    }

    /// The result drawn at `row`, if the rows still hold one there.
    pub fn hit(&self, row: i32) -> Option<Hit> {
        let state = self.0.borrow();
        usize::try_from(row)
            .ok()
            .and_then(|row| state.found.get(row).cloned())
    }

    /// Fill the search's page with what OpenSubtitles found, or say why not.
    pub fn found(&self, ui: &MainWindow, playing: Option<&MediaPath>, found: Found) {
        let find = ui.global::<Find>();
        show_account(&find, found.account.as_ref());
        let media = dbm_naming::parse(found.hits.path().as_str());
        let Some(hits) = found.hits.if_playing(playing) else {
            return;
        };
        match hits {
            Ok(hits) => {
                let rows: Vec<FoundItem> = hits.iter().map(|hit| found_row(hit, &media)).collect();
                find.set_hits(ModelRc::new(VecModel::from(rows)));
                find.set_state(if hits.is_empty() {
                    FindState::NotFound
                } else {
                    FindState::Found
                });
                self.0.borrow_mut().found = hits;
            }
            Err(why) => {
                find.set_said(sentence(&why).into());
                find.set_state(FindState::Failed);
            }
        }
    }

    /// Load a fetched subtitle and show it, or say why there is none.
    pub fn fetched(
        &self,
        ui: &MainWindow,
        mpv: &Mpv,
        playing: Option<&MediaPath>,
        fetched: Fetched,
    ) {
        let find = ui.global::<Find>();
        find.set_fetching(-1);
        show_account(&find, fetched.account.as_ref());
        let path = fetched.result.path().clone();
        let Some(result) = fetched.result.if_playing(playing) else {
            return;
        };
        match result {
            Ok((file, language)) => {
                commands::add_subtitle(mpv, &file, Some(&language), true);
                // Back to the tracks, where it now stands lit: the answer
                // is the row it arrived as.
                ui.set_find_open(false);
            }
            Err(why) => {
                // Kept to be fetched once the person has signed in, which is
                // the way past it, so the menu goes straight to where that is
                // done, if it is still on the page that asked.
                if why.wants_account() {
                    let row = self.row_of(&fetched.hit);
                    self.0.borrow_mut().waiting = Some(ForFile::new(path, fetched.hit));
                    if ui.invoke_offer_sign_in(row, sentence(&why).into()) {
                        return;
                    }
                }
                super::warn(ui, sentence(&why));
            }
        }
    }

    /// Go on with the download that asked for an account, now there is one,
    /// or say on the sign-in page why there isn't.
    pub fn signed_in(
        &self,
        ui: &MainWindow,
        worker: &Worker,
        playing: Option<&MediaPath>,
        result: Result<Account, Failure>,
    ) {
        let find = ui.global::<Find>();
        find.set_signing_in(false);
        match result {
            Ok(account) => {
                show_account(&find, Some(&account));
                find.set_sign_in_said(Default::default());
                // Back to the search, on the row whose download goes on, if
                // the sign-in's page is still the one showing.
                if ui.get_menu_open() && ui.get_account_open() {
                    ui.invoke_close_account();
                }
                let Some(waiting) = self.0.borrow_mut().waiting.take() else {
                    return;
                };
                if playing != Some(waiting.path()) {
                    return;
                }
                find.set_fetching(self.row_of(waiting.value()));
                opensubtitles::fetch(
                    waiting.path().clone(),
                    waiting.value().clone(),
                    worker.reporter(Completion::Fetched),
                );
            }
            Err(why) => find.set_sign_in_said(sentence(&why).into()),
        }
    }

    /// Where `hit` is in the search's results, or -1 once a search since has
    /// replaced them.
    fn row_of(&self, hit: &Hit) -> i32 {
        self.0
            .borrow()
            .found
            .iter()
            .position(|h| h == hit)
            .map_or(-1, |row| row as i32)
    }
}

/// Why OpenSubtitles said no, in the language spoken now.
fn sentence(why: &Failure) -> String {
    match why {
        Failure::Unreachable => tr("Could not reach OpenSubtitles").to_string(),
        Failure::Refused => tr("OpenSubtitles refused the player's key").to_string(),
        Failure::Spent { signed_in: false } => {
            tr("The downloads without an account are used up for today").to_string()
        }
        Failure::Spent { signed_in: true } => {
            tr("Today's subtitle downloads are used up").to_string()
        }
        Failure::Busy => tr("OpenSubtitles is busy. Try again in a moment").to_string(),
        Failure::WrongPassword => tr("The username or password is wrong").to_string(),
        Failure::Lapsed => tr("The sign-in to OpenSubtitles has run out").to_string(),
        Failure::Said(text) => text.clone(),
    }
}

/// Who is signed in to OpenSubtitles, on the search's page.
fn show_account(find: &Find, account: Option<&Account>) {
    find.set_account(account.map(|a| a.name.as_str()).unwrap_or_default().into());
    find.set_left(
        account
            .and_then(|a| a.left)
            .map_or(-1, |left| left.clamp(0, i64::from(i32::MAX)) as i32),
    );
}

/// One result of a search, as its row says it: the release it was timed
/// against, and beside it the language and whatever sets it apart.
fn found_row(hit: &Hit, playing: &Media) -> FoundItem {
    let mut detail = vec![hit.language.name()];
    if hit.exact {
        detail.push(tr("Exact").to_string());
    }
    if hit.hearing_impaired {
        detail.push("SDH".into());
    }
    if hit.machine {
        detail.push(tr("Machine").to_string());
    }
    FoundItem {
        label: dbm_naming::past_named(&hit.release, playing).into(),
        detail: detail.join(" · ").into(),
        exact: hit.exact,
    }
}
