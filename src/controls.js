import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import { toggleAmbient } from "./ambient.js";

const { getCurrentWindow } = window.__TAURI__.window;

const SEEK_SECONDS = 10;
const DOUBLE_CLICK_DELAY_MS = 250;

// --- Playback actions --------------------------------------------------------

function togglePause() {
    player.togglePause();
}

function toggleMute() {
    player.getProperty("mute", "flag").then((muted) => {
        player.setProperty("mute", !muted);
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
    player.getProperty("panscan", "double").then((panscan) => {
        player.setProperty("panscan", panscan === 1 ? 0 : 1);
    });
}

function seek(seconds) {
    player.seek(seconds, "relative");
}

function playPrevious() {
    player.playlistPrev().then(() => setTimeout(() => player.play(), 100));
}

function playNext() {
    player.playlistNext().then(() => setTimeout(() => player.play(), 100));
}

// --- Button wiring -----------------------------------------------------------

document.getElementById("btn-open").onclick = () => player.openVideoDialog();
document.getElementById("btn-previous").onclick = playPrevious;
document.getElementById("btn-next").onclick = playNext;
document.getElementById("btn-seek-back").onclick = () => seek(-SEEK_SECONDS);
document.getElementById("btn-seek-forward").onclick = () => seek(SEEK_SECONDS);
document.getElementById("btn-play").onclick = togglePause;
document.getElementById("btn-ambient").onclick = toggleAmbient;
document.getElementById("btn-panscan").onclick = togglePanscan;
document.getElementById("btn-mute").onclick = toggleMute;
document.getElementById("btn-fullscreen").onclick = toggleFullscreen;

const volumeSlider = document.getElementById("volume-slider");
volumeSlider.addEventListener("input", () => player.setVolume(Number(volumeSlider.value)));

// --- Click vs double-click ---------------------------------------------------

let clickTimeout;

document.getElementById("video-surface").addEventListener("click", (event) => {
    if (event.detail === 1) {
        clickTimeout = setTimeout(togglePause, DOUBLE_CLICK_DELAY_MS);
    } else if (event.detail === 2) {
        clearTimeout(clickTimeout);
        toggleFullscreen();
    }
});

// --- Keyboard shortcuts ------------------------------------------------------

document.addEventListener("keydown", (e) => {
    // prettier-ignore
    switch (e.code) {
    case "Space":       togglePause();       break;
    case "KeyF":        toggleFullscreen();   break;
    case "KeyM":        toggleMute();         break;
    case "KeyT":        togglePanscan();      break;
    case "ArrowRight":  seek(SEEK_SECONDS);   break;
    case "ArrowLeft":   seek(-SEEK_SECONDS);  break;
  }
});
