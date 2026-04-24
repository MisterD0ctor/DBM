const { getCurrentWebview } = window.__TAURI__.webview;
const { listen } = window.__TAURI__.event;
import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import * as ambient from "./ambient.js";
import * as seekbar from "./seekbar.js";
import * as tracks from "./tracks.js";
import * as playlist from "./playlist.js";
import * as preview from "./preview.js";
import { enableSliderScroll } from "./utils/sliderScroll.js";

// Side-effect imports — these register their own event listeners on import
import "./overlay.js";
import "./controls.js";

enableSliderScroll();

let playlistPos = 0;
let playlistCount = 0;

const stateProperties = [
    { name: "time-pos", format: "double" },
    { name: "percent-pos", format: "double" },
    { name: "duration", format: "double" },
    { name: "filename", format: "string" },
    { name: "pause", format: "flag" },
    { name: "mute", format: "flag" },
    { name: "volume", format: "double" },
    { name: "panscan", format: "double" },
    { name: "sid", format: "string" },
    { name: "aid", format: "string" },
    { name: "border-background", format: "string" },
    { name: "eof-reached", format: "flag" },
    { name: "playlist-pos", format: "double" },
    { name: "playlist-count", format: "double" },
    { name: "keep-open", format: "string" },
];

function updateProperty(name, data) {
    // prettier-ignore
    switch (name) {
    case "time-pos":    ui.setCurrentTime(data);              break;
    case "percent-pos": ui.setProgress(data);                 break;
    case "duration":    ui.setDuration(data);        
                        seekbar.setDuration(data);            break;
    case "filename":    ui.setMediaTitle(data);
                        updateCurrentVideoPath();             break;
    case "pause":       ui.setPause(data);
                        ui.setActivePlaylistItem(playlistPos, data); break;
    case "mute":        ui.setMute(data);                     break;
    case "volume":      ui.setVolume(data);                   break;
    case "panscan":     ui.togglePanscan(data);               break;
    case "sid":         ui.setActiveSubtitleTrack(data);      break;
    case "aid":         ui.setActiveAudioTrack(data);         break;
    case "border-background": ui.toggleAmbient(data === "shader"); 
                              ambient.persistParams();        break;
    case "eof-reached":                                       break;
    case "playlist-pos":   playlistPos = data;
                            ui.setPlaylistNav(playlistPos, playlistCount);
                            ui.setActivePlaylistItem(playlistPos, false); break;
    case "playlist-count": playlistCount = data;
                            ui.setPlaylistNav(playlistPos, playlistCount); break;
    case "keep-open":   ui.toggleAutoplay(data !== "always"); break;
    default: console.warn("Unhandled property:", name);
    }
}

async function updateCurrentVideoPath() {
    try {
        const path = await player.getPath();
        preview.setCurrentVideo(path || null);
    } catch {
        preview.setCurrentVideo(null);
    }
}

preview.init();

// --- Property observation -----------------------------------------------------

player.onPropertyChange(({ name, data }) => updateProperty(name, data));

// --- File loaded (track list is now available) --------------------------------

player.onEvent((event) => {
    if (event.event === "file-loaded") updateState();
});

// --- Sync UI with Rust-side mpv state -----------------------------------------

async function updateState() {
    stateProperties.forEach(async (prop) => {
        const data = await player.getProperty(prop.name, prop.format);
        updateProperty(prop.name, data);
    });

    await tracks.populateTrackListMenu();
    await playlist.populatePlaylistMenu();
}

try {
    updateState();
} catch (err) {
    console.warn("mpv state sync (may still be initializing):", err);
}

ambient.initAmbientMenu();

// --- Window events ------------------------------------------------------------

listen("tauri://resize", () => {
    player.getPercentPos().then((percentPos) => ui.setProgress(percentPos));
    ui.updateMediaTitleOverflow();
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

// --- Blur buttons after click to prevent Space from re-triggering them --------

document.addEventListener("click", (e) => {
    if (e.target.closest("button")) e.target.closest("button").blur();
    if (e.target.closest("input")) e.target.closest("input").blur();
});

// --- Disable right click menu -------------------------------------------------

document.addEventListener("contextmenu", (ev) => ev.preventDefault());
