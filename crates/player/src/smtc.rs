//! Windows' System Media Transport Controls.
//!
//! The panel that appears when you press a volume key, the lock screen, and —
//! the reason this matters — the play/pause and track buttons on a headset or
//! a keyboard. Windows routes those to whichever application registered as
//! playing something; without this the player is invisible to all of it and
//! the buttons reach whatever else happens to be open.
//!
//! Two directions, both narrow:
//!
//! * buttons in, turned into ordinary mpv commands;
//! * what is playing and whether it is paused, out.
//!
//! Registration needs the window handle, which does not exist until the window
//! manager has made the window — the same problem the modal-loop hook has, and
//! solved the same way: it is retried from the frame path until it takes.
//!
//! Linux has no equivalent in the same sense. MPRIS over D-Bus is the
//! counterpart and is a different enough shape — a service name, an object
//! path, a property-change signal — that it is its own piece of work, not a
//! `cfg` arm of this one. Everything here compiles away to nothing there.

use std::sync::Arc;

use crate::mpv::Mpv;
use crate::state::PlayerState;

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub use windows_impl::Controls;

#[cfg(windows)]
mod windows_impl {
    use super::*;

    use std::cell::RefCell;

    /// What the overlay is currently showing, so it is only told about
    /// changes.
    ///
    /// Each update is a cross-process call into the shell. At sixty a second
    /// that would be absurd; nothing here changes more than a few times a
    /// minute.
    #[derive(PartialEq)]
    struct Shown {
        title: String,
        paused: bool,
        has_file: bool,
        /// Whether the skip buttons should be live, which is whether there is
        /// anything to skip to.
        playlist: bool,
    }

    impl Shown {
        fn of(player: &PlayerState) -> Self {
            Self {
                title: player.display_title(),
                paused: player.paused,
                has_file: player.path.is_some(),
                playlist: player.playlist_count > 1,
            }
        }
    }

    use windows::Foundation::TypedEventHandler;
    use windows::Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
        SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

    /// Attempts before the window handle is declared hopeless. Same budget as
    /// the modal-loop hook: a couple of seconds of frames.
    const GIVE_UP_AFTER: u32 = 120;

    pub struct Controls {
        state: RefCell<State>,
    }

    enum State {
        /// Waiting for a window handle to register against.
        Pending { mpv: Arc<Mpv>, attempts: u32 },
        Live {
            controls: SystemMediaTransportControls,
            shown: Option<Shown>,
        },
        /// Registration failed, or the machine wants nothing to do with it.
        /// Nothing is retried and nothing is said again.
        Off,
    }

    impl Controls {
        pub fn new(mpv: Arc<Mpv>) -> Self {
            Self {
                state: RefCell::new(State::Pending { mpv, attempts: 0 }),
            }
        }

        /// Tell the overlay what is playing, registering first if that has not
        /// managed to happen yet.
        ///
        /// Called every frame, because registration has to keep being retried
        /// until the window exists. `moved` says whether any mpv property
        /// actually changed: reading the state costs a title parse and an
        /// allocation, which is not something to do sixty times a second for
        /// an answer that changes a few times an hour.
        pub fn publish(&self, window: &slint::Window, player: &PlayerState, moved: bool) {
            let mut state = self.state.borrow_mut();
            if let State::Pending { mpv, attempts } = &mut *state {
                match register(window, mpv.clone()) {
                    Ok(controls) => {
                        eprintln!("dbm: media keys registered with Windows");
                        *state = State::Live {
                            controls,
                            shown: None,
                        };
                    }
                    Err(e) => {
                        *attempts += 1;
                        if *attempts >= GIVE_UP_AFTER {
                            eprintln!("dbm: media keys unavailable ({e}); headset buttons \
                                       and the volume overlay will not reach the player");
                            *state = State::Off;
                        }
                        return;
                    }
                }
            }
            let State::Live { controls, shown } = &mut *state else {
                return;
            };
            // Nothing moved, and the overlay has already been told once.
            if !moved && shown.is_some() {
                return;
            }

            let now = Shown::of(player);
            if shown.as_ref() == Some(&now) {
                return;
            }
            if let Err(e) = update(controls, &now, shown.as_ref()) {
                // One failed update is not a reason to stop trying; the next
                // change will attempt it again. But do not record it as shown,
                // or the overlay would stay wrong until something else moved.
                eprintln!("dbm: media overlay update failed: {e}");
                return;
            }
            *shown = Some(now);
        }
    }

    /// Push one change. Only what actually differs, since each setter is its
    /// own call into the shell — and the title in particular drags a
    /// `DisplayUpdater` round trip behind it.
    fn update(
        controls: &SystemMediaTransportControls,
        now: &Shown,
        before: Option<&Shown>,
    ) -> windows::core::Result<()> {
        if before.map(|b| b.paused) != Some(now.paused)
            || before.map(|b| b.has_file) != Some(now.has_file)
        {
            controls.SetPlaybackStatus(match (now.has_file, now.paused) {
                // `Closed` is what tells the overlay to stop showing us
                // altogether, which is right when nothing is loaded.
                (false, _) => MediaPlaybackStatus::Closed,
                (true, true) => MediaPlaybackStatus::Paused,
                (true, false) => MediaPlaybackStatus::Playing,
            })?;
        }
        if before.map(|b| b.playlist) != Some(now.playlist) {
            controls.SetIsNextEnabled(now.playlist)?;
            controls.SetIsPreviousEnabled(now.playlist)?;
        }
        if before.map(|b| b.title.as_str()) != Some(now.title.as_str()) {
            let updater = controls.DisplayUpdater()?;
            updater.SetType(MediaPlaybackType::Video)?;
            // The parsed name rather than the file name: the overlay is one
            // line wide and `[Group] Show - 03 [1080p][AV1]` spends most of it
            // on the encode. See `naming`.
            updater.VideoProperties()?.SetTitle(&now.title.as_str().into())?;
            updater.Update()?;
        }
        Ok(())
    }

    /// Claim the controls for this window and wire the buttons up.
    fn register(
        window: &slint::Window,
        mpv: Arc<Mpv>,
    ) -> Result<SystemMediaTransportControls, String> {
        let controls = for_window(window).map_err(|e| e.to_string())?;

        // `IsEnabled` is what makes Windows show us at all. Play and pause are
        // always sensible; the skip buttons are switched on and off with the
        // playlist by `update`.
        (|| -> windows::core::Result<()> {
            controls.SetIsEnabled(true)?;
            controls.SetIsPlayEnabled(true)?;
            controls.SetIsPauseEnabled(true)?;
            controls.SetIsNextEnabled(false)?;
            controls.SetIsPreviousEnabled(false)?;
            controls.SetPlaybackStatus(MediaPlaybackStatus::Closed)
        })()
        .map_err(|e| format!("configuring the controls: {e}"))?;

        controls
            .ButtonPressed(&TypedEventHandler::<
                SystemMediaTransportControls,
                SystemMediaTransportControlsButtonPressedEventArgs,
            >::new(move |_, args| {
                if let Some(button) = args.as_ref().and_then(|a| a.Button().ok()) {
                    pressed(&mpv, button);
                }
                Ok(())
            }))
            .map_err(|e| format!("registering for button presses: {e}"))?;

        Ok(controls)
    }

    fn for_window(window: &slint::Window) -> windows::core::Result<SystemMediaTransportControls> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let provider = window.window_handle();
        let handle = provider
            .window_handle()
            .map_err(|_| windows::core::Error::from_win32())?;
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return Err(windows::core::Error::from_win32());
        };
        let hwnd = HWND(win32.hwnd.get() as *mut core::ffi::c_void);

        let interop = windows::core::factory::<
            SystemMediaTransportControls,
            ISystemMediaTransportControlsInterop,
        >()?;
        // Per-window rather than per-process, which is why this needs the
        // handle at all.
        unsafe { interop.GetForWindow(hwnd) }
    }

    /// One button, as an mpv command.
    ///
    /// **Runs on a Windows thread pool thread**, not the UI thread. That is
    /// fine and deliberate: mpv's commands are thread-safe and asynchronous,
    /// so the press is queued without touching anything of ours. Marshalling
    /// it to the UI thread would only add a hop — and a button pressed while
    /// the interface is asleep would wait for a frame that is not coming.
    fn pressed(mpv: &Mpv, button: SystemMediaTransportControlsButton) {
        use crate::commands;
        match button {
            SystemMediaTransportControlsButton::Play => commands::set_pause(mpv, false),
            SystemMediaTransportControlsButton::Pause => commands::set_pause(mpv, true),
            // Stepping and playing, the same thing the button at the end of
            // a file does.
            SystemMediaTransportControlsButton::Next => commands::advance(mpv, 1),
            SystemMediaTransportControlsButton::Previous => commands::advance(mpv, -1),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Everywhere else
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub use other::Controls;

#[cfg(not(windows))]
mod other {
    use super::*;

    pub struct Controls;

    impl Controls {
        pub fn new(_mpv: Arc<Mpv>) -> Self {
            Self
        }

        /// No-op until there is an MPRIS implementation to put here.
        pub fn publish(&self, _window: &slint::Window, _player: &PlayerState, _moved: bool) {}
    }
}
