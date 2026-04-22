/**
 * Player module — thin JS layer over Rust-side mpv management.
 *
 * mpv is initialized on the Rust side during app setup.
 * This module only sends commands and subscribes to events.
 */

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ---------------------------------------------------------------------------
// Video loading
// ---------------------------------------------------------------------------

export function loadVideo(path) {
    return invoke("load_video", { path });
}

export function openVideoDialog() {
    return invoke("open_video_dialog");
}

export function openFolderDialog() {
    return invoke("open_folder_dialog");
}

// ---------------------------------------------------------------------------
// Playback commands
// ---------------------------------------------------------------------------

export function play() {
    return invoke("play");
}

export function pause() {
    return invoke("pause");
}

export function togglePause() {
    return invoke("toggle_pause");
}

/**
 * @param {number} target
 * @param {"absolute"|"relative"|"absolute-percent"|"relative-percent"} [mode="absolute"]
 * @param {"keyframes"|"exact"} [precision="keyframes"]
 */
export function seek(target, mode = "absolute", precision = "exact") {
    return invoke("seek", { target, mode, precision });
}

export function setVolume(volume) {
    return invoke("set_volume", { volume });
}

export function setSpeed(speed) {
    return invoke("set_speed", { speed });
}

// ---------------------------------------------------------------------------
// Playlist navigation
// ---------------------------------------------------------------------------

export function playlistPlayIndex(index) {
    return invoke("playlist_play_index", { index });
}

export function playlistPrev() {
    return invoke("playlist_prev");
}

export function playlistNext() {
    return invoke("playlist_next");
}

// ---------------------------------------------------------------------------
// Generic property access (escape hatch)
// ---------------------------------------------------------------------------

export function setProperty(name, value) {
    return invoke("set_property", { name, value });
}

export function getProperty(name, format = "string") {
    return invoke("get_property", { name, format });
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Watch-later progress
// ---------------------------------------------------------------------------

/**
 * @param {string[]} paths
 * @returns {Promise<Record<string, {start: number, duration: number}>>}
 */
export function getWatchLaterPositions(paths) {
    return invoke("get_watch_later_positions", { paths });
}

/**
 * Fetch a shader from frontend assets and apply it as the border-background.
 * @param {string} assetPath e.g. "assets/shaders/ambient-lite.glsl"
 */
export async function setBorderShader(assetPath) {
    const response = await fetch(assetPath);
    if (!response.ok) throw new Error(`Failed to fetch shader: ${response.status}`);
    const content = await response.text();
    return invoke("apply_border_shader", { content });
}

/**
 *
 * @param {{ name: string, value: any }[]} params
 * @returns
 */
export function setBorderShaderOptions(opts) {
    opts = opts.map(({ name, value }) => `${name}=${value}`).join(",");
    return invoke("set_border_shader_options", { opts });
}

// Events from Rust (property changes, file-end, errors)
// ---------------------------------------------------------------------------

/**
 * Subscribe to mpv property changes.
 * Callback receives `{ name: string, data: any }`.
 *
 * @param {(event: {name: string, data: any}) => void} callback
 * @returns {Promise<() => void>} unlisten function
 */
export function onPropertyChange(callback) {
    return listen("mpv://property", (event) => callback(event.payload));
}

/**
 * Subscribe to file-end events.
 * Callback receives `{ reason: string }` — "eof" | "stop" | "error".
 *
 * @param {(event: {reason: string}) => void} callback
 * @returns {Promise<() => void>} unlisten function
 */
export function onFileEnd(callback) {
    return listen("mpv://file-end", (event) => callback(event.payload));
}

/**
 * Subscribe to generic mpv events (catch-all).
 *
 * @param {(event: any) => void} callback
 * @returns {Promise<() => void>} unlisten function
 */
export function onEvent(callback) {
    return listen("mpv://event", (event) => callback(event.payload));
}
