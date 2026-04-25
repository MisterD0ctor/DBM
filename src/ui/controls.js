import { setButtonIcon, setButtonTooltip } from "../utils/setButtonIcon.js";

export function toggleOverlay(force) {
    document.getElementById("controls-panel")?.classList.toggle("hidden", !force);
    document.getElementById("player")?.classList.toggle("hidden", !force);
}

export function toggleAmbient(force) {
    document.getElementById("ambient-toggle")?.classList.toggle("on", force);
}

export function toggleAmbientMenu(force) {
    const menu = document.getElementById("ambient-menu");
    if (force !== undefined) menu?.classList.toggle("hidden", !force);
    else menu?.classList.toggle("hidden");
}

export function togglePanscan(force) {
    setButtonIcon("btn-panscan", force ? "assets/icons/compress-alt.svg" : "assets/icons/expand-alt.svg");
    setButtonTooltip("btn-panscan", force ? "Fit" : "Cover");
}

export function setPlaylistNav(pos, count) {
    const prev = document.getElementById("btn-previous");
    const next = document.getElementById("btn-next");
    prev.classList.toggle("disabled", pos <= 0);
    next.classList.toggle("disabled", pos >= count - 1);
    prev.disabled = pos <= 0;
    next.disabled = pos >= count - 1;
}

export function toggleFullscreen(force) {
    setButtonIcon("btn-fullscreen", force ? "assets/icons/compress.svg" : "assets/icons/expand.svg");
    setButtonTooltip("btn-fullscreen", force ? "Exit Fullscreen" : "Fullscreen");
}

export function toggleAutoplay(force) {
    setButtonIcon(
        "btn-autoplay",
        force ? "assets/icons/arrow-right-slash.svg" : "assets/icons/arrow-right.svg",
    );
    setButtonTooltip("btn-autoplay", force ? "Disable Autoplay" : "Enable Autoplay");
}

export function toggleOpenMenu(force) {
    const openMenu = document.getElementById("open-menu");
    const openMenuBtn = document.getElementById("btn-open-menu");
    if (force !== undefined) {
        openMenu?.classList.toggle("hidden", !force);
        openMenuBtn?.classList.toggle("rotated", force);
    } else {
        openMenu?.classList.toggle("hidden");
        openMenuBtn?.classList.toggle("rotated");
    }
}
