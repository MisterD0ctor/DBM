import * as player from "./player.js";
import * as ui from "./ui/ui.js";

const { getCurrentWindow } = window.__TAURI__.window;
const { PhysicalPosition, PhysicalSize, LogicalPosition, LogicalSize } = window.__TAURI__.dpi;

const SEEK_SECONDS = 10;
const DOUBLE_CLICK_DELAY_MS = 250;

// --- Playback actions --------------------------------------------------------

async function togglePause() {
    await player.togglePause();
    return await player.getProperty("pause", "flag");
}

async function toggleMute() {
    const muted = await player.getProperty("mute", "flag");
    await player.setProperty("mute", !muted);
    return !muted;
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

    ui.toggleFullscreen(enable);
}

function toggleFullscreen() {
    getCurrentWindow()
        .isFullscreen()
        .then((isFs) => setFullscreen(!isFs));
}

async function togglePanscan() {
    const panscan = await player.getProperty("panscan", "double");
    await player.setProperty("panscan", panscan === 1 ? 0 : 1);
    return !panscan;
}

function seek(seconds) {
    player.seek(seconds, "relative");
}

function seekBackward() {
    seek(-SEEK_SECONDS);
    ui.showPlaybackOverlay("seek-backward");
}

function seekForward() {
    seek(SEEK_SECONDS);
    ui.showPlaybackOverlay("seek-forward");
}

function rewind() {
    player.seek(0, "absolute");
    player.play();
}

function playPrevious() {
    player.playlistPrev().then(() => setTimeout(() => player.play(), 100));
}

function playNext() {
    player.playlistNext().then(() => setTimeout(() => player.play(), 100));
}

// --- Autoplay ----------------------------------------------------------------

let autoplayEnabled = 1;

function toggleAutoplay() {
    autoplayEnabled = !autoplayEnabled;

    ui.toggleAutoplay(autoplayEnabled);

    if (autoplayEnabled) {
        player.setProperty("keep-open", "always");
        player.setProperty("reset-on-next-file", "pause");
    } else {
        player.setProperty("keep-open", "no");
        player.setProperty("reset-on-next-file", "no");
    }
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

document.getElementById("btn-rewind").onclick = rewind;
document.getElementById("btn-previous").onclick = playPrevious;
document.getElementById("btn-next").onclick = playNext;
document.getElementById("btn-autoplay").onclick = toggleAutoplay;
document.getElementById("btn-seek-back").onclick = () => seekBackward();
document.getElementById("btn-seek-forward").onclick = () => seekForward();
document.getElementById("btn-play").onclick = togglePause;
document.getElementById("btn-panscan").onclick = togglePanscan;
document.getElementById("btn-mute").onclick = toggleMute;
document.getElementById("btn-fullscreen").onclick = toggleFullscreen;

const volumeSlider = document.getElementById("volume-slider");
volumeSlider.addEventListener("input", () => player.setVolume(Number(volumeSlider.value)));

// --- Click vs double-click ---------------------------------------------------

let clickTimeout;

document.getElementById("video-surface").addEventListener("click", (event) => {
    if (event.detail === 1) {
        clickTimeout = setTimeout(
            () =>
                togglePause().then((state) =>
                    ui.showPlaybackOverlay("pause-" + (state ? "on" : "off")),
                ),
            DOUBLE_CLICK_DELAY_MS,
        );
    } else if (event.detail === 2) {
        clearTimeout(clickTimeout);
        toggleFullscreen();
    }
});

// --- Button press visual feedback ---------------------------------------------------

document.querySelectorAll("button").forEach((btn) => {
    btn.addEventListener("mousedown", () => btn.classList.add("pressed"));
    btn.addEventListener("mouseup", () => btn.classList.remove("pressed"));
    btn.addEventListener("mouseleave", () => btn.classList.remove("pressed"));
});

// --- Keyboard shortcuts ------------------------------------------------------

document.addEventListener("keydown", (e) => {
    // prettier-ignore
    switch (e.code) {
    case "Escape":      setFullscreen(false); break;
    case "Space":       togglePause()
                            .then((state) => ui.showPlaybackOverlay("pause-" + (state ? "on" : "off"))); break;
    case "F11":
    case "KeyF":        toggleFullscreen();   break;
    case "KeyM":        toggleMute()
                            .then((state) => ui.showPlaybackOverlay("mute-" + (state ? "on" : "off"))); break;
    case "KeyT":        togglePanscan()
                            .then((state) => ui.showPlaybackOverlay("panscan-" + (state ? "on" : "off"))); break;
    case "ArrowRight":  e.ctrlKey ? playNext()     : seekForward();  break;
    case "ArrowLeft":   e.ctrlKey ? playPrevious() : seekBackward(); break;
    }
});
