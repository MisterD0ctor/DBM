//! Rust wrapper around `libmpv-wrapper.dll`.
//!
//! Owns the FFI boundary and the single `MpvPlayer` instance. All app-facing
//! interaction goes through `commands` (Tauri-invokable) or `properties`
//! (typed getters). Events come out via `events::event_callback` and are
//! emitted as typed [`shared::MpvProperty`] / [`shared::MpvEvent`].

pub mod commands;
pub mod events;
pub mod properties;

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::Mutex;

use log::{info, trace, warn};
use once_cell::sync::OnceCell;
use raw_window_handle::HasWindowHandle;
use serde::Deserialize;
use shared::{MpvErrorDto, MpvErrorKind};
use tauri::{AppHandle, Manager};

use events::EventUserData;

const APP_DATA_DIR: &str = "Death by MPV";

// ---------------------------------------------------------------------------
// FFI wrapper types (mirrors libmpv-wrapper.dll exports)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvHandle {
    _unused: [u8; 0],
}

type EventCallback = Option<unsafe extern "C" fn(event: *const c_char, userdata: *mut c_void)>;

struct LibmpvWrapper {
    _library: libloading::Library,
    mpv_wrapper_create: unsafe extern "C" fn(
        initial_options: *const c_char,
        observed_properties: *const c_char,
        event_callback: EventCallback,
        event_userdata: *mut c_void,
    ) -> *mut MpvHandle,
    mpv_wrapper_destroy: unsafe extern "C" fn(handle: *mut MpvHandle),
    mpv_wrapper_command: unsafe extern "C" fn(
        handle: *mut MpvHandle,
        name: *const c_char,
        args: *const c_char,
    ) -> *mut c_char,
    mpv_wrapper_set_property: unsafe extern "C" fn(
        handle: *mut MpvHandle,
        name: *const c_char,
        value: *const c_char,
    ) -> *mut c_char,
    mpv_wrapper_get_property: unsafe extern "C" fn(
        handle: *mut MpvHandle,
        name: *const c_char,
        format: *const c_char,
    ) -> *mut c_char,
    mpv_wrapper_free: unsafe extern "C" fn(s: *mut c_char),
}

impl LibmpvWrapper {
    unsafe fn load(path: &str) -> Result<Self, MpvError> {
        let library = unsafe { libloading::Library::new(path) }?;

        unsafe {
            Ok(Self {
                mpv_wrapper_create: *library.get(b"mpv_wrapper_create\0")?,
                mpv_wrapper_destroy: *library.get(b"mpv_wrapper_destroy\0")?,
                mpv_wrapper_command: *library.get(b"mpv_wrapper_command\0")?,
                mpv_wrapper_set_property: *library.get(b"mpv_wrapper_set_property\0")?,
                mpv_wrapper_get_property: *library.get(b"mpv_wrapper_get_property\0")?,
                mpv_wrapper_free: *library.get(b"mpv_wrapper_free\0")?,
                _library: library,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Error type — rich internal variants; collapses to `MpvErrorDto` at the IPC
// boundary so the frontend can deserialize it.
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum MpvError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("Failed to get window handle: {0}")]
    WindowHandle(#[from] raw_window_handle::HandleError),
    #[error("FFI error: {0}")]
    Ffi(String),
    #[error("Failed to create mpv instance")]
    CreateInstance,
    #[error("mpv instance not initialized")]
    NotInitialized,
    #[error(transparent)]
    Libloading(#[from] libloading::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    NulError(#[from] std::ffi::NulError),
    #[error("mpv command failed: {0}")]
    Command(String),
    #[error("mpv set property failed: {0}")]
    SetProperty(String),
    #[error("mpv get property failed: {0}")]
    GetProperty(String),
}

pub type MpvResult<T> = Result<T, MpvError>;

impl MpvError {
    pub fn into_dto(self) -> MpvErrorDto {
        let kind = match &self {
            MpvError::NotInitialized => MpvErrorKind::NotInitialized,
            MpvError::Command(_) => MpvErrorKind::Command,
            MpvError::SetProperty(_) => MpvErrorKind::SetProperty,
            MpvError::GetProperty(_) => MpvErrorKind::GetProperty,
            MpvError::Io(_) => MpvErrorKind::Io,
            MpvError::Ffi(_)
            | MpvError::WindowHandle(_)
            | MpvError::CreateInstance
            | MpvError::Libloading(_)
            | MpvError::NulError(_) => MpvErrorKind::Ffi,
            MpvError::Tauri(_) | MpvError::SerdeJson(_) => MpvErrorKind::Other,
        };
        MpvErrorDto {
            kind,
            message: self.to_string(),
        }
    }
}

// Helper for `?`-flavored conversion at the Tauri command boundary.
pub(crate) fn to_dto<T>(r: MpvResult<T>) -> Result<T, MpvErrorDto> {
    r.map_err(MpvError::into_dto)
}

// ---------------------------------------------------------------------------
// FFI response parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FfiResponse {
    data: Option<serde_json::Value>,
    error: Option<String>,
}

// ---------------------------------------------------------------------------
// Instance data (handle + event callback userdata)
// ---------------------------------------------------------------------------

struct MpvInstance {
    handle: *mut MpvHandle,
    event_userdata: *mut c_void,
}

unsafe impl Send for MpvInstance {}
unsafe impl Sync for MpvInstance {}

// ---------------------------------------------------------------------------
// MpvPlayer — the single managed state
// ---------------------------------------------------------------------------

pub struct MpvPlayer {
    wrapper: OnceCell<LibmpvWrapper>,
    instance: Mutex<Option<MpvInstance>>,
}

unsafe impl Send for MpvPlayer {}
unsafe impl Sync for MpvPlayer {}

/// Initial mpv options applied at creation time.
const INITIAL_OPTIONS: &[(&str, &str)] = &[
    ("vo", "gpu-next"),
    ("hwdec", "auto-safe"),
    ("keep-open", "always"),
    ("force-window", "yes"),
    ("pause", "no"),
    ("deband", "yes"),
    ("deband-iterations", "8"),
    ("sub-visibility", "yes"),
    ("save-position-on-quit", "yes"),
    // `sub-delay` rides along so a file whose subtitles needed nudging into
    // sync opens that way next time instead of being re-fixed every session.
    ("watch-later-options", "start,vid,aid,sid,volume,sub-delay"),
];

/// Properties to observe — the event loop pushes changes to the frontend as
/// typed [`shared::MpvProperty`] variants. Keep this list in sync with the
/// enum variants in `shared`.
const OBSERVED_PROPERTIES: &[(&str, &str)] = &[
    ("filename", "string"),
    ("path", "string"),
    ("duration", "double"),
    ("time-pos", "double"),
    ("percent-pos", "double"),
    ("pause", "flag"),
    ("mute", "flag"),
    ("volume", "double"),
    ("sid", "string"),
    ("aid", "string"),
    ("sub-visibility", "flag"),
    ("sub-delay", "double"),
    ("sub-scale", "double"),
    ("sub-pos", "double"),
    ("track-list/count", "double"),
    ("eof-reached", "flag"),
    ("playlist-pos", "double"),
    ("playlist-count", "double"),
    ("border-background", "string"),
    ("panscan", "double"),
    // Not forwarded to the frontend — `mpv::events` routes it to the audio
    // watchdog. Observing it is also what makes mpv monitor device hotplug
    // at all, which is the point: see `crate::audio`.
    ("audio-device-list", "node"),
];

impl Default for MpvPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl MpvPlayer {
    pub fn new() -> Self {
        Self {
            wrapper: OnceCell::new(),
            instance: Mutex::new(None),
        }
    }

    // --- Lifecycle -----------------------------------------------------------

    /// Initialize mpv, embed into the main window, start observing properties.
    pub fn init(&self, app: &AppHandle) -> MpvResult<()> {
        let wrapper = self.load_wrapper()?;

        let mut guard = self.instance.lock().unwrap();
        if guard.is_some() {
            info!("mpv instance already exists, skipping init");
            return Ok(());
        }

        let mut opts = serde_json::Map::new();
        for &(k, v) in INITIAL_OPTIONS {
            opts.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }

        // Point mpv at our app-data watch-later directory.
        let watch_later_dir = app_data_dir().join("watch_later");
        let _ = std::fs::create_dir_all(&watch_later_dir);
        opts.insert(
            "watch-later-directory".to_string(),
            serde_json::Value::String(watch_later_dir.to_string_lossy().into_owned()),
        );

        // Embed mpv into the Tauri window via platform `wid`.
        let window = app
            .get_webview_window("main")
            .ok_or_else(|| MpvError::Ffi("window 'main' not found".into()))?;
        let wh = window.window_handle()?;
        let wid = get_wid(wh.as_raw())?;
        opts.insert("wid".to_string(), serde_json::json!(wid));

        let opts_json = serde_json::to_string(&opts)?;

        let obs: serde_json::Map<String, serde_json::Value> = OBSERVED_PROPERTIES
            .iter()
            .map(|&(name, fmt)| (name.to_string(), serde_json::Value::String(fmt.to_string())))
            .collect();
        let obs_json = serde_json::to_string(&obs)?;

        let c_opts = CString::new(opts_json)?;
        let c_obs = CString::new(obs_json)?;

        let free_fn = wrapper.mpv_wrapper_free;
        let event_data = Box::new(EventUserData {
            app: app.clone(),
            free_fn,
        });
        let event_userdata = Box::into_raw(event_data) as *mut c_void;

        let handle = unsafe {
            (wrapper.mpv_wrapper_create)(
                c_opts.as_ptr(),
                c_obs.as_ptr(),
                Some(events::event_callback),
                event_userdata,
            )
        };

        if handle.is_null() {
            let _ = unsafe { Box::from_raw(event_userdata as *mut EventUserData) };
            return Err(MpvError::CreateInstance);
        }

        info!("mpv instance initialized");

        // mpv distinguishes "options" (set at creation) from "properties" (set
        // at runtime). A handful of options need to be re-applied as
        // properties to actually take effect — apply them all defensively.
        for &(k, v) in INITIAL_OPTIONS {
            if let Err(e) = self.set_property_raw_with(wrapper, handle, k, v) {
                warn!("Failed to set initial property '{}': {}", k, e);
            }
        }

        *guard = Some(MpvInstance {
            handle,
            event_userdata,
        });

        Ok(())
    }

    /// Save mpv's watch-later state for the current file.
    pub fn write_watch_later(&self) -> MpvResult<()> {
        self.command("write-watch-later-config", &[])
    }

    /// Destroy the mpv instance and free resources.
    pub fn destroy(&self) -> MpvResult<()> {
        if let Err(e) = self.write_watch_later() {
            warn!("Failed to save watch-later config: {}", e);
        }

        let mut guard = self.instance.lock().unwrap();
        if let Some(instance) = guard.take() {
            let wrapper = self.load_wrapper()?;
            unsafe {
                (wrapper.mpv_wrapper_destroy)(instance.handle);
                let _ = Box::from_raw(instance.event_userdata as *mut EventUserData);
            }
            info!("mpv instance destroyed");
        }
        Ok(())
    }

    // --- Low-level operations ------------------------------------------------

    pub fn command(&self, name: &str, args: &[serde_json::Value]) -> MpvResult<()> {
        trace!("mpv command '{}' args={:?}", name, args);

        let wrapper = self.load_wrapper()?;
        let guard = self.instance.lock().unwrap();
        let instance = guard.as_ref().ok_or(MpvError::NotInitialized)?;

        let args_json = serde_json::to_string(args)?;
        let c_name = CString::new(name)?;
        let c_args = CString::new(args_json)?;

        let result_ptr = unsafe {
            (wrapper.mpv_wrapper_command)(instance.handle, c_name.as_ptr(), c_args.as_ptr())
        };

        self.parse_void_response(wrapper, result_ptr, MpvError::Command)
    }

    pub fn set_property_raw(&self, name: &str, value: &str) -> MpvResult<()> {
        trace!("mpv set '{}' = '{}'", name, value);

        let wrapper = self.load_wrapper()?;
        let guard = self.instance.lock().unwrap();
        let instance = guard.as_ref().ok_or(MpvError::NotInitialized)?;

        self.set_property_raw_with(wrapper, instance.handle, name, value)
    }

    fn set_property_raw_with(
        &self,
        wrapper: &LibmpvWrapper,
        handle: *mut MpvHandle,
        name: &str,
        value: &str,
    ) -> MpvResult<()> {
        let c_name = CString::new(name)?;
        let c_value = CString::new(serde_json::to_string(&serde_json::Value::String(
            value.to_string(),
        ))?)?;

        let result_ptr = unsafe {
            (wrapper.mpv_wrapper_set_property)(handle, c_name.as_ptr(), c_value.as_ptr())
        };

        self.parse_void_response(wrapper, result_ptr, MpvError::SetProperty)
    }

    pub fn set_property_value(&self, name: &str, value: &serde_json::Value) -> MpvResult<()> {
        trace!("mpv set '{}' = {:?}", name, value);

        let wrapper = self.load_wrapper()?;
        let guard = self.instance.lock().unwrap();
        let instance = guard.as_ref().ok_or(MpvError::NotInitialized)?;

        let c_name = CString::new(name)?;
        let c_value = CString::new(serde_json::to_string(value)?)?;

        let result_ptr = unsafe {
            (wrapper.mpv_wrapper_set_property)(instance.handle, c_name.as_ptr(), c_value.as_ptr())
        };

        self.parse_void_response(wrapper, result_ptr, MpvError::SetProperty)
    }

    pub fn get_property(&self, name: &str, format: &str) -> MpvResult<serde_json::Value> {
        let wrapper = self.load_wrapper()?;
        let guard = self.instance.lock().unwrap();
        let instance = guard.as_ref().ok_or(MpvError::NotInitialized)?;

        let c_name = CString::new(name)?;
        let c_format = CString::new(format)?;

        let result_ptr = unsafe {
            (wrapper.mpv_wrapper_get_property)(instance.handle, c_name.as_ptr(), c_format.as_ptr())
        };

        if result_ptr.is_null() {
            return Err(MpvError::GetProperty("FFI returned null".into()));
        }

        let response_str = unsafe { CStr::from_ptr(result_ptr).to_string_lossy().to_string() };
        unsafe { (wrapper.mpv_wrapper_free)(result_ptr) };

        let response: FfiResponse = serde_json::from_str(&response_str)?;

        if let Some(err) = response.error {
            return Err(MpvError::GetProperty(err));
        }

        response
            .data
            .ok_or_else(|| MpvError::GetProperty("no data in response".into()))
    }

    // --- Helpers -------------------------------------------------------------

    fn parse_void_response<F>(
        &self,
        wrapper: &LibmpvWrapper,
        result_ptr: *mut c_char,
        make_err: F,
    ) -> MpvResult<()>
    where
        F: FnOnce(String) -> MpvError,
    {
        if result_ptr.is_null() {
            return Err(MpvError::Ffi("FFI returned null".into()));
        }

        let response_str = unsafe { CStr::from_ptr(result_ptr).to_string_lossy().to_string() };
        unsafe { (wrapper.mpv_wrapper_free)(result_ptr) };

        let response: FfiResponse = serde_json::from_str(&response_str)?;

        if let Some(err) = response.error {
            Err(make_err(err))
        } else {
            Ok(())
        }
    }

    fn load_wrapper(&self) -> MpvResult<&LibmpvWrapper> {
        self.wrapper.get_or_try_init(|| {
            info!("Loading libmpv-wrapper...");

            let lib_name = "libmpv-wrapper.dll";

            let mut search_dirs: Vec<PathBuf> = Vec::new();
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    search_dirs.push(exe_dir.to_path_buf());
                    search_dirs.push(exe_dir.join("lib"));
                }
            }

            let lib_path = search_dirs
                .iter()
                .map(|dir| dir.join(lib_name))
                .find(|path| path.exists())
                .unwrap_or_else(|| PathBuf::from(lib_name));

            // Add the DLL's directory to the Win32 search path so libmpv-2.dll
            // (a transitive dep of libmpv-wrapper.dll) resolves at load time.
            #[cfg(target_os = "windows")]
            if let Some(dir) = lib_path.parent() {
                use std::os::windows::ffi::OsStrExt;
                let wide: Vec<u16> = dir
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                unsafe {
                    let _ = windows::Win32::System::LibraryLoader::SetDllDirectoryW(
                        windows::core::PCWSTR(wide.as_ptr()),
                    );
                }
            }

            let lib_path_str = lib_path.to_string_lossy().into_owned();
            info!("Loading from: {}", lib_path_str);
            let wrapper = unsafe { LibmpvWrapper::load(&lib_path_str)? };
            info!("libmpv-wrapper loaded successfully");
            Ok(wrapper)
        })
    }
}

// ---------------------------------------------------------------------------
// App data directory
// ---------------------------------------------------------------------------

pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DATA_DIR)
}

// ---------------------------------------------------------------------------
// Platform helper: extract window id for embedding
// ---------------------------------------------------------------------------

fn get_wid(raw: raw_window_handle::RawWindowHandle) -> MpvResult<i64> {
    match raw {
        raw_window_handle::RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as i64),
        raw_window_handle::RawWindowHandle::Xlib(handle) => Ok(handle.window as i64),
        raw_window_handle::RawWindowHandle::Xcb(handle) => Ok(handle.window.get() as i64),
        raw_window_handle::RawWindowHandle::AppKit(handle) => Ok(handle.ns_view.as_ptr() as i64),
        _ => Err(MpvError::Ffi(
            "Unsupported platform for window embedding".into(),
        )),
    }
}
