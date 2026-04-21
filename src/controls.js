import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import { toggleAmbient } from "./ambient.js";

const { getCurrentWindow } = window.__TAURI__.window;
const { PhysicalPosition, PhysicalSize, LogicalPosition, LogicalSize } = window.__TAURI__.dpi;

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

// --- Borderless fullscreen with animation ------------------------------------

let savedWindowState = {
    isMaximized: false,
    position: undefined,
    size: undefined,
};

async function setFullscreen(enable) {
    const win = getCurrentWindow();

    if (enable) {
        savedWindowState = {
            isMaximized: await win.isMaximized(),
            position: await win.outerPosition(),
            size: await win.innerSize(),
        };
        await win.maximize();
        await win.setFullscreen(true);
    } else {
        await win.setFullscreen(false);
        if (!savedWindowState.isMaximized) {
            await win.setSize(savedWindowState.size); // Sets inner size
            await win.setPosition(savedWindowState.position); // Sets outer position
            await win.unmaximize();
        }
    }

    ui.setFullscreen(enable);
}

function toggleFullscreen() {
    getCurrentWindow()
        .isFullscreen()
        .then((isFs) => setFullscreen(!isFs));
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

// document.getElementById("btn-open-file").onclick = () => player.openVideoDialog();
// document.getElementById("btn-open-folder").onclick = () => player.openFolderDialog();

const openMenu = document.getElementById("open-menu");
const openMenuBtn = document.getElementById("btn-open-menu");

openMenuBtn.onclick = () => {
    ui.toggleOpenMenu();
};

openMenu.addEventListener("click", (e) => {
    const item = e.target.closest(".menu-item");
    if (!item) return;
    ui.toggleOpenMenu(false);
    if (item.dataset.action === "open-file") player.openVideoDialog();
    else if (item.dataset.action === "open-folder") player.openFolderDialog();
});

document.addEventListener("click", (event) => {
    if (!openMenu.contains(event.target) && !openMenuBtn.contains(event.target)) {
        ui.toggleOpenMenu(false);
    }
});

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
    case "Escape":      setFullscreen(false); break;
    case "Space":       togglePause();        break;
    case "F11":
    case "KeyF":        toggleFullscreen();   break;
    case "KeyM":        toggleMute();         break;
    case "KeyT":        togglePanscan();      break;
    case "ArrowRight":  e.ctrlKey ? playNext()          : seek(SEEK_SECONDS);  break;
    case "ArrowLeft":   e.ctrlKey ? playPrevious()      : seek(-SEEK_SECONDS); break;
    }
});
