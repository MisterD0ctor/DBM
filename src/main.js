const { listen } = window.__TAURI__.event;
import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import * as ambient from "./ambient.js";
import * as tracks from "./tracks.js";
import * as playlist from "./playlist.js";
import * as preview from "./preview.js";
import { enableSliderScroll } from "./utils/sliderScroll.js";
import { initToolbarOverflow } from "./utils/toolbarOverflow.js";

// Side-effect imports — these register their own event listeners on import
import "./seekbar.js";
import "./overlay.js";
import "./controls.js";
import "./openMenu.js";
import "./endOfPlayback.js";
import "./dragDrop.js";

enableSliderScroll();
initToolbarOverflow();

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
    { name: "sub-visibility", format: "flag" },
    { name: "border-background", format: "string" },
    { name: "eof-reached", format: "flag" },
    { name: "playlist-pos", format: "double" },
    { name: "playlist-count", format: "double" },
];

function updateProperty(name, data) {
    // prettier-ignore
    switch (name) {
    case "time-pos":          ui.setCurrentTime(data);                     break;
    case "percent-pos":       ui.setProgress(data);                        break;
    case "duration":          ui.setDuration(data);                        break;
    case "filename":          ui.setMediaTitle(data);
                              ui.setPlaylistButtonVisible(!!data);
                              preview.refreshCurrentVideo();               break;
    case "pause":             ui.setPause(data);
                              ui.setActivePlaylistItem(playlistPos, data); break;
    case "mute":              ui.setMute(data);                            break;
    case "volume":            ui.setVolume(data);                          break;
    case "panscan":           ui.togglePanscan(data);                      break;
    case "sid":               ui.setActiveSubtitleTrackID(data);           break;
    case "aid":               ui.setActiveAudioTrackID(data);              break;
    case "sub-visibility":    ui.setSubtitleVisibility(data);              break;
    case "border-background": ambient.applyState(data);                    break;
    case "eof-reached":       ui.setEndOfPlayback(data);                   break;
    case "track-list/count":  tracks.populateTrackListMenu();              break;
    case "playlist-pos":      playlistPos = data;
                              ui.setPlaylistNav(playlistPos, playlistCount);
                              ui.setActivePlaylistItem(playlistPos, false);
                              ui.setIsLastVideo(playlistPos >= playlistCount - 1); break;
    case "playlist-count":    playlistCount = data;
                              ui.setPlaylistNav(playlistPos, playlistCount);
                              ui.setIsLastVideo(playlistPos >= playlistCount - 1); break;
    default:                  console.warn("Unhandled property:", name);
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
    ui.refreshTimeDisplays();
});

listen("tauri://open-file", (event) => {
    if (event.payload) {
        player.loadVideo(event.payload);
    }
});

// --- Blur buttons after click to prevent Space from re-triggering them --------

document.addEventListener("click", (e) => {
    if (e.target.closest("button")) e.target.closest("button").blur();
    if (e.target.closest("input")) e.target.closest("input").blur();
});

// --- Disable right click menu -------------------------------------------------

document.addEventListener("contextmenu", (ev) => ev.preventDefault());
