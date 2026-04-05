const { getCurrentWebview } = window.__TAURI__.webview;
const { getCurrentWindow } = window.__TAURI__.window;
const { resolveResource } = window.__TAURI__.path;
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
import { isVideoFile } from "./utils/isVideoFile.js";
import { lerp } from "./utils/lerp.js";
import * as mpv from "./libmpv/libmpv.js";
import * as ui from "./ui/ui.js";
import * as smtc from "./smtc/smtc.js";

// --- Constants ----------------------------------------------------------------

const SEEK_SECONDS = 10;
const OVERLAY_HIDE_DELAY_MS = 3000;
const DOUBLE_CLICK_DELAY_MS = 250;

const OBSERVED_PROPERTIES = [
    ["time-pos", "double", "none"],
    ["percent-pos", "double", "none"],
    ["duration", "double", "none"],
    ["filename", "string", "none"],
    ["pause", "flag"],
    ["mute", "flag"],
    ["volume", "double"],
    ["speed", "double"],
    ["eof-reached", "flag", "none"],
    ["panscan", "double", "none"],
    ["sid", "string", "none"],
    ["aid", "string", "none"],
];

// --- Variables ----------------------------------------------------------------

let duration = 0;

// --- mpv init -----------------------------------------------------------------

const mpvConfig = {
    initialOptions: {
        "border-background": "blur",
        "background-blur-radius": "50",
        deband: "yes",
        "deband-iterations": "8",
        "force-window": "yes",
        hwdec: "auto-safe",
        "keep-open": "yes",
        pause: "no",
        "sub-visibility": "yes",
        sid: "no",
        vo: "gpu-next",
    },
    observedProperties: OBSERVED_PROPERTIES,
};

try {
    console.log("MPV loading");
    await mpv.init(mpvConfig);
    await Object.keys(mpvConfig.initialOptions).forEach(async (key) => {
        await mpv.setProperty(key, mpvConfig.initialOptions[key]);
    });

    const videoPath = await invoke("get_startup_file");
    loadVideo(videoPath);

    setAmbientAspectRatio();
    console.log("MPV initialized");
} catch (err) {
    console.error("MPV initialization failed:", err);
}

// --- Property observation -----------------------------------------------------

mpv.observeProperties(OBSERVED_PROPERTIES, ({ name, data }) => {
    // prettier-ignore
    switch (name) {
    case "time-pos":     onTimePosChanged(data);    break;
    case "percent-pos":  onPercentPosChanged(data); break;
    case "duration":     onDurationChanged(data);   break;
    case "filename":     onFilenameChanged(data);   break;
    case "pause":        onPauseChanged(data);      break;
    case "mute":         onMuteChanged(data);       break;
    case "volume":       onVolumeChanged(data);     break;
    case "speed":        onSpeedChanged(data);      break;
    case "panscan":      onPanscanChanged(data);    break;
    case "eof-reached":  onEOFReached(data);        break;
    case "sid":          onSIDChanged(data);        break;
    case "aid":          onAIDChanged(data);        break;
    default: console.warn("Unhandled property:", name);
  }
});

function onTimePosChanged(timePos) {
    ui.setCurrentTime(timePos);
}

function onPercentPosChanged(percentPos) {
    ui.setProgress(percentPos);
}

function onDurationChanged(data) {
    duration = data;
    ui.setDuration(data);
}

function onFilenameChanged(filename) {
    ui.setFilename(filename);
    smtc.setMetadata(filename);
    populateTrackListMenu();
}

function onPauseChanged(pause) {
    ui.setPause(pause);
    smtc.setPlayback(!pause);
}

function onMuteChanged(mute) {
    ui.setMute(mute);
}

function onVolumeChanged(volume) {
    ui.setVolume(volume);
}

function onSpeedChanged(speed) {}

function onPanscanChanged(panscan) {
    ui.setPanscan(panscan);
    setAmbientAspectRatio();
}

function onEOFReached(data) {}

function onSIDChanged(sid) {
    ui.setSelectedSubtitleTrack(sid);
}

function onAIDChanged(aid) {
    ui.setSelectedAudioTrack(aid);
}

// --- Window event listeners -------------------------------------------------

listen("tauri://resize", () => {
    setAmbientAspectRatio();
});

listen("tauri://open-file", (event) => {
    loadVideo(event.payload);
});

// --- Playlist helpers ---------------------------------------------------------

function loadVideo(videoPath) {
    if (videoPath && isVideoFile(videoPath)) {
        invoke("playlist_from_video", { videoPath }).then(([playlistPath, fileIndex]) => {
            loadPlaylist(playlistPath, fileIndex);
        });
    }
}

function loadPlaylist(playlistPath, fileIndex) {
    mpv.command("loadlist", [playlistPath, "replace"]).then(() => {
        mpv.command("playlist-play-index", [fileIndex]);
        // Small delay to allow the file to load before unpausing / refreshing track-list
        setTimeout(() => {
            mpv.setProperty("pause", "no");
        }, 100);
    });
}

async function populateTrackListMenu() {
    const trackListString = await mpv.getProperty("track-list", "string");
    const trackList = JSON.parse(trackListString);
    const subtitle = trackList.filter((t) => t.type === "sub");
    const audio = trackList.filter((t) => t.type === "audio");

    const selectedSubtitleId = subtitle.find((t) => t.selected)?.id ?? "no";
    const selectedAudioId = audio.find((t) => t.selected)?.id;

    ui.populateSubtitleTrackMenu(
        subtitle,
        (id) =>
            mpv
                .setProperty("sid", id.toString())
                .then(() => mpv.setProperty("sub-visibility", "yes")),
        () => mpv.setProperty("sid", "no"),
    );

    ui.populateAudioTrackMenu(audio, (id) => {
        mpv.setProperty("aid", id.toString());
    });

    ui.setSelectedSubtitleTrack(selectedSubtitleId);
    ui.setSelectedAudioTrack(selectedAudioId);
}

// --- Drag & drop --------------------------------------------------------------

getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const videoPath = event.payload.paths[0]?.toString();
    loadVideo(videoPath);
});

// --- Overlay show/hide -------------------------------------------------------

let hideTimer = null;

function showOverlay() {
    ui.setOverlay(true);
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => ui.setOverlay(false), OVERLAY_HIDE_DELAY_MS);
}

const overlay = document.getElementById("overlay");
overlay.addEventListener("mousemove", showOverlay);
overlay.addEventListener("mouseenter", showOverlay);
overlay.addEventListener("mousedown", showOverlay);
overlay.addEventListener("mouseleave", () => ui.setOverlay(false));

// --- Playback controls --------------------------------------------------------

function pause() {
    mpv.setProperty("pause", "yes");
}

function play() {
    mpv.setProperty("pause", "no");
}

function togglePause() {
    mpv.getProperty("pause", "bool").then((paused) => {
        mpv.setProperty("pause", paused === "yes" ? "no" : "yes");
    });
}

function toggleMute() {
    mpv.getProperty("mute", "bool").then((muted) => {
        mpv.setProperty("mute", muted === "yes" ? "no" : "yes");
    });
}

function toggleFullscreen() {
    getCurrentWindow()
        .isFullscreen()
        .then((isFs) => {
            getCurrentWindow().setFullscreen(!isFs);
            ui.setFullscreen(!isFs);
        });
}

function togglePanscan() {
    mpv.getProperty("panscan", "double").then((panscan) => {
        const next = panscan === 1 ? 0 : 1;
        mpv.setProperty("panscan", next);
    });
}

function seek(seconds) {
    mpv.getProperty("time-pos", "double").then((pos) => {
        mpv.setProperty("time-pos", pos + seconds);
    });
}

function playPrevious() {
    mpv.command("playlist-prev").then(() =>
        setTimeout(() => {
            mpv.setProperty("pause", "no");
        }, 100),
    );
}

function playNext() {
    mpv.command("playlist-next").then(() =>
        setTimeout(() => {
            mpv.setProperty("pause", "no");
        }, 100),
    );
}

// --- Ambient lighting ---------------------------------------------------------

let ambientEnabled = true;
ui.setAmbientOverlay(ambientEnabled);

function toggleAmbient() {
    ambientEnabled = !ambientEnabled;

    if (ambientEnabled) {
        ui.setAmbientOverlay(true);
        mpv.setProperty("border-background", "blur");
    } else {
        ui.setAmbientOverlay(false);
        mpv.setProperty("border-background", "color");
    }

    setAmbientAspectRatio();
}

function setAmbientAspectRatio() {
    mpv.getProperty("video-params/aspect", "double")
        .catch(() => 0)
        .then((aspect) => {
            mpv.getProperty("panscan", "double").then((panscan) => {
                const uiAspect = ui.getAspectRatio();
                const effectiveAspect = lerp(aspect, uiAspect, panscan);

                ui.setAmbientAspectRatio(effectiveAspect);
            });
        });
}

// --- Button wiring ------------------------------------------------------------

document.getElementById("open-path-button").onclick = () =>
    invoke("playlist_from_path_dialog").then(([playlistPath, fileIndex]) =>
        loadPlaylist(playlistPath, fileIndex),
    );

document.getElementById("previous-button").onclick = playPrevious;
document.getElementById("next-button").onclick = playNext;
document.getElementById("seek-backward-button").onclick = () => seek(-SEEK_SECONDS);
document.getElementById("seek-forward-button").onclick = () => seek(SEEK_SECONDS);
document.getElementById("play-button").onclick = togglePause;
document.getElementById("ambient-button").onclick = toggleAmbient;
document.getElementById("panscan-button").onclick = togglePanscan;
document.getElementById("mute-button").onclick = toggleMute;
document.getElementById("fullscreen-button").onclick = toggleFullscreen;

const volumeSlider = document.getElementById("volume-slider");
volumeSlider.addEventListener("input", () => mpv.setProperty("volume", volumeSlider.value));

// --- Track list menu --------------------------------------------------------------

const trackListMenu = document.getElementById("track-list-menu");
const trackListButton = document.getElementById("track-list-button");

trackListButton.onclick = () => {
    populateTrackListMenu();
    ui.toggleTrackListMenu();
};

document.addEventListener("click", (event) => {
    if (!trackListMenu.contains(event.target) && !trackListButton.contains(event.target)) {
        event.preventDefault();
        ui.hideTracksMenu();
    }
});

// --- Seek bar -----------------------------------------------------------------

const seekEl = document.querySelector(".seek");

seekEl.addEventListener("click", (event) => {
    const percent = (event.offsetX / seekEl.getBoundingClientRect().width) * 100;
    mpv.setProperty("percent-pos", percent);
});

seekEl.addEventListener("mousemove", (event) => {
    const fraction = event.offsetX / seekEl.getBoundingClientRect().width;
    ui.setSeekTimeHighlight(true, fraction * duration, fraction * 100);
});

seekEl.addEventListener("mouseleave", () => ui.setSeekTimeHighlight(false));

overlay.addEventListener("mousemove", () => {
    if (!seekEl.matches(":hover")) ui.setSeekTimeHighlight(false);
});

// --- Click vs double-click ----------------------------------------------------

let clickTimeout;

document.querySelector(".interactive").addEventListener("click", (event) => {
    if (event.detail === 1) {
        clickTimeout = setTimeout(togglePause, DOUBLE_CLICK_DELAY_MS);
    } else if (event.detail === 2) {
        clearTimeout(clickTimeout);
        toggleFullscreen();
    }
});

// --- Keyboard shortcuts -------------------------------------------------------

document.addEventListener("keydown", (e) => {
    // prettier-ignore
    switch (e.code) {
    case "Space":       togglePause();       break;
    case "KeyF":        toggleFullscreen();  break;
    case "KeyM":        toggleMute();        break;
    case "KeyT":        togglePanscan();     break;
    case "ArrowRight":  seek(SEEK_SECONDS);  break;
    case "ArrowLeft":   seek(-SEEK_SECONDS); break;
  }
});

// --- Windows System Media Transport Controls (SMTC) ---------------------------

smtc.setup((cmd) => {
    // prettier-ignore
    switch (cmd) {
        case "play":     play(); break;
        case "pause":    pause(); break;
        case "next":     playNext();     break;
        case "previous": playPrevious(); break;
    }
});
