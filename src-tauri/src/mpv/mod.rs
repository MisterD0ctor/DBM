pub mod commands;
pub mod events;
pub mod properties;

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::Mutex;

use log::{info, trace, warn};
use once_cell::sync::OnceCell;
use raw_window_handle::HasWindowHandle;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// App data subdirectory name for watch-later files and session state.
const APP_DATA_DIR: &str = "Death by MPV";

use events::EventUserData;

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
            let create = *library.get(b"mpv_wrapper_create\0")?;
            let destroy = *library.get(b"mpv_wrapper_destroy\0")?;
            let command = *library.get(b"mpv_wrapper_command\0")?;
            let set_prop = *library.get(b"mpv_wrapper_set_property\0")?;
            let get_prop = *library.get(b"mpv_wrapper_get_property\0")?;
            let free = *library.get(b"mpv_wrapper_free\0")?;

            Ok(Self {
                _library: library,
                mpv_wrapper_create: create,
                mpv_wrapper_destroy: destroy,
                mpv_wrapper_command: command,
                mpv_wrapper_set_property: set_prop,
                mpv_wrapper_get_property: get_prop,
                mpv_wrapper_free: free,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
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

impl Serialize for MpvError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type MpvResult<T> = Result<T, MpvError>;

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
    ("keep-open", "yes"),
    ("force-window", "yes"),
    ("pause", "no"),
    ("deband", "yes"),
    ("deband-iterations", "8"),
    ("border-background", "blur"),
    ("background-blur-radius", "50"),
    ("sub-visibility", "yes"),
    ("sid", "no"),
    // Watch Later — save/restore position, tracks, volume across sessions
    ("save-position-on-quit", "yes"),
    ("watch-later-options", "start,vid,aid,sid,volume"),
];

/// Properties to observe — the event loop will push changes to the frontend.
const OBSERVED_PROPERTIES: &[(&str, &str)] = &[
    ("time-pos", "double"),
    ("percent-pos", "double"),
    ("duration", "double"),
    ("filename", "string"),
    ("pause", "flag"),
    ("mute", "flag"),
    ("volume", "double"),
    ("speed", "double"),
    ("eof-reached", "flag"),
    ("panscan", "double"),
    ("sid", "string"),
    ("aid", "string"),
    ("border-background", "string"),
];

impl MpvPlayer {
    pub fn new() -> Self {
        Self {
            wrapper: OnceCell::new(),
            instance: Mutex::new(None),
        }
    }

    // --- Lifecycle -----------------------------------------------------------

    /// Initialize mpv, embed into the given window, start observing properties.
    pub fn init(&self, app: &AppHandle) -> MpvResult<()> {
        let wrapper = self.load_wrapper()?;

        let mut guard = self.instance.lock().unwrap();
        if guard.is_some() {
            info!("mpv instance already exists, skipping init");
            return Ok(());
        }

        // Build initial options JSON including wid
        let mut opts = serde_json::Map::new();
        for &(k, v) in INITIAL_OPTIONS {
            opts.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }

        // Set watch-later directory inside app data
        let watch_later_dir = app_data_dir().join("watch_later");
        let _ = std::fs::create_dir_all(&watch_later_dir);
        opts.insert(
            "watch-later-directory".to_string(),
            serde_json::Value::String(watch_later_dir.to_string_lossy().into_owned()),
        );

        // Embed mpv into the Tauri window
        let window = app
            .get_webview_window("main")
            .ok_or_else(|| MpvError::Ffi("window 'main' not found".into()))?;
        let wh = window.window_handle()?;
        let raw = wh.as_raw();
        let wid = get_wid(raw)?;
        opts.insert("wid".to_string(), serde_json::json!(wid));

        let opts_json = serde_json::to_string(&opts)?;

        // Build observed properties JSON { "name": "format", ... }
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

        // Apply initial options as properties too (for options that need runtime set)
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
    /// Saves watch-later config before destroying.
    pub fn destroy(&self) -> MpvResult<()> {
        // Save watch-later before tearing down
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

        self.parse_void_response(wrapper, result_ptr, |msg| MpvError::Command(msg))
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

        self.parse_void_response(wrapper, result_ptr, |msg| MpvError::SetProperty(msg))
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

        self.parse_void_response(wrapper, result_ptr, |msg| MpvError::SetProperty(msg))
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

            #[cfg(target_os = "windows")]
            let lib_name = "libmpv-wrapper.dll";
            #[cfg(target_os = "macos")]
            let lib_name = "libmpv-wrapper.dylib";
            #[cfg(target_os = "linux")]
            let lib_name = "libmpv-wrapper.so";

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

            // Add the DLL's directory to the search path so that its own
            // dependencies (e.g. libmpv-2.dll) are found at runtime.
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
// App data directory + last-session persistence
// ---------------------------------------------------------------------------

pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DATA_DIR)
}

fn last_session_path() -> PathBuf {
    app_data_dir().join("last_session.txt")
}

/// Save the currently playing file path so it can be resumed next launch.
pub fn save_last_session(path: &str) {
    let file = last_session_path();
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    if let Err(e) = std::fs::write(&file, path) {
        warn!("Failed to save last session: {}", e);
    }
}

/// Read the last session's file path (if any).
pub fn load_last_session() -> Option<String> {
    let file = last_session_path();
    std::fs::read_to_string(&file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && PathBuf::from(s).is_file())
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
