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

// --- Property observation -----------------------------------------------------

await player.onPropertyChange(({ name, data }) => {
    // prettier-ignore
    switch (name) {
    case "time-pos":    ui.setCurrentTime(data);           break;
    case "percent-pos": ui.setProgress(data);              break;
    case "duration":    ui.setDuration(data);     
                        seekbar.setDuration(data);         break;
    case "filename":    ui.setFilename(data);
                        ui.setLoaded(true);                break;
    case "pause":       ui.setPause(data);                 break;
    case "mute":        ui.setMute(data);                  break;
    case "volume":      ui.setVolume(data);                break;
    case "panscan":     ui.setPanscan(data);
                        ambient.updateAspectRatio();       break;
    case "sid":         ui.setSelectedSubtitleTrack(data); break;
    case "aid":         ui.setSelectedAudioTrack(data);    break;
    case "border-background": ui.setAmbient(data === "blur"); break;
    default: console.warn("Unhandled property:", name);
    }
});

// --- File loaded (track list is now available) --------------------------------

player.onEvent((event) => {
    if (event.event === "file-loaded") {
        populateTrackListMenu();
        ambient.updateAspectRatio();
    }
});

// --- Sync UI with Rust-side mpv state -----------------------------------------

try {
    const state = await player.getState();
    seekbar.setDuration(state.duration);
    ui.setDuration(state.duration);
    ui.setCurrentTime(state.time_pos);
    ui.setProgress(state.percent_pos);
    ui.setPause(state.paused);
    ui.setMute(state.mute);
    ui.setVolume(state.volume);
    ui.setPanscan(state.panscan);
    ui.setAmbient(state.border_background === "blur");
    if (state.filename) {
        populateTrackListMenu();
        ambient.updateAspectRatio();
        ui.setFilename(state.filename);
        ui.setLoaded(true);
    }
    console.log(state);
} catch (err) {
    console.warn("mpv state sync (may still be initializing):", err);
}

// --- Window events ------------------------------------------------------------

listen("tauri://resize", () => ambient.updateAspectRatio());

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
