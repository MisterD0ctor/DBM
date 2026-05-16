import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import * as ambient from "./ambient.js";
import * as endOfPlayback from "./endOfPlayback.js";
import { setFullscreen, toggleFullscreen } from "./fullscreen.js";
import { rewind, playPrevious, playNext } from "./navigation.js";
import { refreshToolbarOverflow } from "./utils/toolbarOverflow.js";

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

async function togglePanscan() {
    const panscan = await player.getPanscan();
    player.setPanscan(panscan === 1 ? 0 : 1);
    return !panscan;
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

function bumpVolume(delta) {
    player
        .changeVolume(delta)
        .then(() => player.getVolume())
        .then((volume) => ui.showActionOverlay("volume", `${volume}%`));
}

// --- Button wiring -----------------------------------------------------------

document.getElementById("btn-previous").onclick = playPrevious;
document.getElementById("btn-next").onclick = playNext;
document.getElementById("btn-seek-back").onclick = () => seekBackward();
document.getElementById("btn-seek-forward").onclick = () => seekForward();
document.getElementById("btn-play").onclick = () => {
    if (endOfPlayback.isAtEnd()) endOfPlayback.restart();
    else togglePause();
};
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
            if (endOfPlayback.isAtEnd()) {
                endOfPlayback.restart();
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
            bumpVolume(2);
            break;
        case "ArrowDown":
            bumpVolume(-2);
            break;
        case "Home":
            rewind();
            break;
        case "ArrowRight":
            if (endOfPlayback.isAtEnd()) {
                endOfPlayback.advance();
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
