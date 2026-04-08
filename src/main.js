const { getCurrentWebview } = window.__TAURI__.webview;
const { listen } = window.__TAURI__.event;
import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import * as ambient from "./ambient.js";
import * as seekbar from "./seekbar.js";
import { populateTrackListMenu } from "./tracks.js";

// Side-effect imports — these register their own event listeners on import
import "./overlay.js";
import "./controls.js";

let playlistPos = 0;
let playlistCount = 0;

// --- Property observation -----------------------------------------------------

await player.onPropertyChange(({ name, data }) => {
    // prettier-ignore
    switch (name) {
    case "time-pos":    break;
    case "percent-pos": break;
    default: console.log(`Property change: {${name}: ${data}}`);
    }

    // prettier-ignore
    switch (name) {
    case "time-pos":    ui.setCurrentTime(data);              break;
    case "percent-pos": ui.setProgress(data);                 break;
    case "duration":    ui.setDuration(data);        
                        seekbar.setDuration(data);            break;
    case "filename":    ui.setMediaTitle(data);               break;
    case "pause":       ui.setPause(data);                    break;
    case "mute":        ui.setMute(data);                     break;
    case "volume":      ui.setVolume(data);                   break;
    case "panscan":     ui.setPanscan(data);   
                        ambient.updateAspectRatio();          break;
    case "sid":         ui.setSelectedSubtitleTrack(data);    break;
    case "aid":         ui.setSelectedAudioTrack(data);       break;
    case "border-background": ui.setAmbient(data === "blur"); break;
    case "eof-reached":                                       break;
    case "playlist-pos":   playlistPos = data ?? 0;
                           ui.setPlaylistNav(playlistPos, playlistCount); break;
    case "playlist-count": playlistCount = data ?? 0;
                           ui.setPlaylistNav(playlistPos, playlistCount); break;
    default: console.warn("Unhandled property:", name);
    }
});

// --- File loaded (track list is now available) --------------------------------

player.onEvent((event) => {
    if (event.event === "file-loaded") {
        updateState();
    }
});

// --- Sync UI with Rust-side mpv state -----------------------------------------

async function updateState() {
    const state = await player.getState();
    console.log(state);

    ui.setDuration(state.duration);
    seekbar.setDuration(state.duration);
    ui.setCurrentTime(state.time_pos);
    ui.setProgress(state.percent_pos);
    ui.setPause(state.paused);
    ui.setMute(state.mute);
    ui.setVolume(state.volume);
    ui.setPanscan(state.panscan);
    ui.setAmbient(state.border_background === "blur");
    ui.setMediaTitle(state.filename);
    [playlistPos, playlistCount] = [state.playlist_pos, state.playlist_count];
    ui.setPlaylistNav(playlistPos, playlistCount);
    populateTrackListMenu();
    ambient.updateAspectRatio();
}

try {
    updateState();
} catch (err) {
    console.warn("mpv state sync (may still be initializing):", err);
}

// --- Window events ------------------------------------------------------------

listen("tauri://resize", () => {
    player.getProperty("percent-pos", "double").then((percentPos) => ui.setProgress(percentPos));
    ambient.updateAspectRatio();
});

listen("tauri://open-file", (event) => {
    if (event.payload) {
        player.loadVideo(event.payload);
    }
});

// --- Drag & drop --------------------------------------------------------------

getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const videoPath = event.payload.paths[0]?.toString();
    if (videoPath) {
        player.loadVideo(videoPath);
    }
});

// --- Disable right click menu -------------------------------------------------

// document.addEventListener("contextmenu", (ev) => ev.preventDefault());
