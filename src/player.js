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
    return invoke("set_property", { name: "volume", value: volume });
}

/** Adjust the current volume by `delta` (can be negative). */
export async function changeVolume(delta) {
    const current = (await getVolume()) ?? 0;
    return setVolume(Math.max(0, Math.min(150, current + delta)));
}

export function setSpeed(speed) {
    return invoke("set_property", { name: "speed", value: speed });
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
// Generic property access (escape hatch — prefer the typed wrappers below)
// ---------------------------------------------------------------------------

export function setProperty(name, value) {
    return invoke("set_property", { name, value });
}

export function getProperty(name, format = "string") {
    return invoke("get_property", { name, format });
}

// ---------------------------------------------------------------------------
// Typed property accessors
// ---------------------------------------------------------------------------

/** @returns {Promise<boolean>} */
export function getPause() {
    return invoke("get_property", { name: "pause", format: "flag" });
}
/** @param {boolean} paused */
export function setPause(paused) {
    return invoke("set_property", { name: "pause", value: paused });
}

/** @returns {Promise<boolean>} */
export function getMute() {
    return invoke("get_property", { name: "mute", format: "flag" });
}
/** @param {boolean} muted */
export function setMute(muted) {
    return invoke("set_property", { name: "mute", value: muted });
}

/** @returns {Promise<number>} */
export function getVolume() {
    return invoke("get_property", { name: "volume", format: "double" });
}

/** @returns {Promise<number>} */
export function getPanscan() {
    return invoke("get_property", { name: "panscan", format: "double" });
}
/** @param {number} value */
export function setPanscan(value) {
    return invoke("set_property", { name: "panscan", value });
}

/** @returns {Promise<string>} */
export function getKeepOpen() {
    return invoke("get_property", { name: "keep-open", format: "string" });
}
/** @param {"yes"|"no"|"always"} value */
export function setKeepOpen(value) {
    return invoke("set_property", { name: "keep-open", value });
}

/** @param {"yes"|"no"|"pause"} value */
export function setResetOnNextFile(value) {
    return invoke("set_property", { name: "reset-on-next-file", value });
}

/** @returns {Promise<string | null>} */
export function getPath() {
    return invoke("get_property", { name: "path", format: "string" });
}

/** @returns {Promise<number>} */
export function getPercentPos() {
    return invoke("get_property", { name: "percent-pos", format: "double" });
}

/** @returns {Promise<number>} */
export function getPlaylistPos() {
    return invoke("get_property", { name: "playlist-pos", format: "double" });
}

/** @returns {Promise<Array<{filename: string, current?: boolean, playing?: boolean}>>} */
export async function getPlaylist() {
    const raw = await invoke("get_property", { name: "playlist", format: "string" });
    return typeof raw === "string" ? JSON.parse(raw) : raw;
}

/** @returns {Promise<Array<{id: number, type: string, selected?: boolean, title?: string, lang?: string}>>} */
export async function getTrackList() {
    const raw = await invoke("get_property", { name: "track-list", format: "string" });
    return typeof raw === "string" ? JSON.parse(raw) : raw;
}

/** @returns {Promise<string>} */
export function getBorderBackground() {
    return invoke("get_property", { name: "border-background", format: "string" });
}
/** @param {"shader"|"color"} value */
export function setBorderBackground(value) {
    return invoke("set_property", { name: "border-background", value });
}

/** @param {string} id e.g. "1", "no", "auto" */
export function setSid(id) {
    return invoke("set_property", { name: "sid", value: id });
}
/** @param {string} id e.g. "1", "no", "auto" */
export function setAid(id) {
    return invoke("set_property", { name: "aid", value: id });
}

/** @param {boolean} visible */
export function setSubVisibility(visible) {
    return invoke("set_property", { name: "sub-visibility", value: visible });
}

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
 *
 * @param {{ name: string, value: any }[]} params
 * @returns
 */
export function setBorderShaderOptions(opts) {
    opts = opts.map(({ name, value }) => `${name}=${value}`).join(",");
    return invoke("set_border_shader_options", { opts });
}

/** @param {Record<string, number>} values */
export function saveAmbientParams(values) {
    return invoke("save_ambient_params", { values });
}

/** @returns {Promise<Record<string, number> | null>} */
export function loadAmbientParams() {
    return invoke("load_ambient_params");
}

/**
 * Fetch any cached seek-preview sprite for the given video path.
 * @param {string} path
 * @returns {Promise<null | {path: string, sprite: string, grid: number, tile_w: number, tile_h: number}>}
 */
export function getPreview(path) {
    return invoke("get_preview", { path });
}

/** Subscribe to preview-ready events from the Rust side. */
export function onPreviewReady(callback) {
    return listen("preview://ready", (event) => callback(event.payload));
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
