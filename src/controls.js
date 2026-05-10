import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import * as ambient from "./ambient.js";

const { getCurrentWindow } = window.__TAURI__.window;
const { PhysicalPosition, PhysicalSize, LogicalPosition, LogicalSize } = window.__TAURI__.dpi;

const SEEK_SECONDS = 10;
const DOUBLE_CLICK_DELAY_MS = 250;

// --- Playback actions --------------------------------------------------------

async function togglePause() {
    await player.togglePause();
    return await player.getPause();
}

async function toggleMute() {
    const muted = await player.getMute();
    await player.setMute(!muted);
    return !muted;
}

async function toggleSubtitles() {
    const visible = await player.getSubVisibility();
    await player.setSubVisibility(!visible);
    return !visible;
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
    const panscan = await player.getPanscan();
    await player.setPanscan(panscan === 1 ? 0 : 1);
    return !panscan;
}

async function toggleAmbient() {
    const ambient = await player.getAmbient();
    await player.setAmbient(!ambient);
    return !ambient;
}

function seek(seconds) {
    player.seek(seconds, "relative");
}

function seekBackward() {
    seek(-SEEK_SECONDS);
}

function seekForward() {
    seek(SEEK_SECONDS);
}

function rewind() {
    player.seek(0, "absolute");
    player.play();
}

// When the user explicitly advances (next/prev), they want the new file to
// start fresh — override mpv's watch-later resume position by seeking to 0
// once the file is loaded.
let resetOnNextLoad = false;

player.onEvent((event) => {
    if (event.event !== "file-loaded") return;
    if (atEnd) exitEnd();
    if (resetOnNextLoad) {
        resetOnNextLoad = false;
        player.seek(0, "absolute");
    }
});

function playPrevious() {
    resetOnNextLoad = true;
    player.playlistPrev().then(() => setTimeout(() => player.play(), 100));
}

function playNext() {
    resetOnNextLoad = true;
    player.playlistNext().then(() => setTimeout(() => player.play(), 100));
}

// --- End-of-playback state ---------------------------------------------------

// When the current file ends, mpv emits eof-reached. The play button becomes a
// "restart" button, ArrowRight advances to the next file (or rewinds on the
// last file), and a centered popup gives the user a click target.
let atEnd = false;
let playlistPos = 0;
let playlistCount = 0;

const isLastVideo = () => playlistCount > 0 && playlistPos >= playlistCount - 1;

player.onPropertyChange(({ name, data }) => {
    if (name === "eof-reached") atEnd = !!data;
    else if (name === "playlist-pos") playlistPos = data ?? 0;
    else if (name === "playlist-count") playlistCount = data ?? 0;
    else if (name === "pause" && data === false && atEnd) exitEnd();
});

function exitEnd() {
    atEnd = false;
    ui.setEndOfPlayback(false);
}

function advanceFromEnd() {
    exitEnd();
    if (isLastVideo()) rewind();
    else playNext();
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
document.getElementById("btn-seek-back").onclick = () => seekBackward();
document.getElementById("btn-seek-forward").onclick = () => seekForward();
document.getElementById("btn-play").onclick = () => {
    if (atEnd) {
        exitEnd();
        rewind();
    } else {
        togglePause().then((state) => ui.showActionOverlay("pause-" + (state ? "on" : "off")));
    }
};
document.getElementById("end-of-playback")?.addEventListener("click", advanceFromEnd);
document.getElementById("btn-panscan").onclick = togglePanscan;
document.getElementById("btn-mute").onclick = toggleMute;
document.getElementById("btn-fullscreen").onclick = toggleFullscreen;

const volumeSlider = document.getElementById("volume-slider");
volumeSlider.addEventListener("input", (e) => {
    let value = Number(volumeSlider.value);
    // Snap to 100 only on real drag input — synthesized events from wheel
    // scrolling are not trusted and shouldn't be snapped.
    if (e.isTrusted && Math.abs(value - 100) <= 6) {
        value = 100;
        volumeSlider.value = 100;
    }
    player.setVolume(value);
});

// --- Click vs double-click ---------------------------------------------------

let clickTimeout;

document.getElementById("video-surface").addEventListener("click", (event) => {
    // If a menu is open, this click should just dismiss it — don't also
    // toggle pause or fullscreen. The menu's own click-outside listener
    // handles the close on the same event.
    if (document.querySelector(".menu:not(.hidden)")) return;

    if (event.detail === 1) {
        clickTimeout = setTimeout(
            () =>
                togglePause().then((state) =>
                    ui.showActionOverlay("pause-" + (state ? "on" : "off")),
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
    switch (e.code) {
        case "Escape":
            setFullscreen(false);
            break;
        case "Space":
            if (atEnd) {
                exitEnd();
                rewind();
            } else {
                togglePause().then((state) =>
                    ui.showActionOverlay("pause-" + (state ? "on" : "off")),
                );
            }
            break;
        case "F11":
        case "KeyF":
            toggleFullscreen();
            break;
        case "KeyM":
            toggleMute().then((state) => ui.showActionOverlay("mute-" + (state ? "on" : "off")));
            break;
        case "KeyT":
            togglePanscan().then((state) =>
                ui.showActionOverlay("panscan-" + (state ? "on" : "off")),
            );
            break;
        case "KeyC":
            toggleSubtitles().then((state) =>
                ui.showActionOverlay("subtitles-" + (state ? "on" : "off")),
            );
            break;
        case "KeyB":
            ambient
                .toggleAmbient()
                .then((state) => ui.showActionOverlay("ambient-" + (state ? "on" : "off")));
            break;
        case "ArrowUp":
            player
                .changeVolume(2)
                .then(() => player.getVolume())
                .then((volume) => {
                    ui.showActionOverlay("volume", `${volume}%`);
                });
            break;
        case "ArrowDown":
            player
                .changeVolume(-2)
                .then(() => player.getVolume())
                .then((volume) => {
                    ui.showActionOverlay("volume", `${volume}%`);
                });
            break;
        case "Home":
            rewind();
            break;
        case "ArrowRight":
            if (atEnd) {
                advanceFromEnd();
            } else if (e.ctrlKey) {
                playNext();
            } else {
                seekForward();
                ui.showActionOverlay("seek-forward", "", 80);
            }
            break;
        case "ArrowLeft":
            if (e.ctrlKey) {
                playPrevious();
            } else {
                seekBackward();
                ui.showActionOverlay("seek-backward", "", 20);
            }
            break;
    }
});
