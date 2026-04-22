import { setButtonIcon } from "../utils/setButtonIcon.js";

export function toggleOverlay(force) {
    document.getElementById("controls-panel")?.classList.toggle("hidden", !force);
    document.getElementById("player")?.classList.toggle("hidden", !force);
}

export function toggleAmbient(force) {
    document.getElementById("ambient-toggle")?.classList.toggle("on", force);
    setButtonIcon(
        "btn-ambient",
        force
            ? "assets/icons/normal-straight/lightbulb-slash.svg"
            : "assets/icons/normal-straight/bulb.svg",
    );
}

export function toggleAmbientMenu(force) {
    const menu = document.getElementById("ambient-menu");
    if (force !== undefined) menu?.classList.toggle("hidden", !force);
    else menu?.classList.toggle("hidden");
}

export function togglePanscan(force) {
    setButtonIcon(
        "btn-panscan",
        force
            ? "assets/icons/normal-straight/compress-alt.svg"
            : "assets/icons/normal-straight/expand-alt.svg",
    );
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
    setButtonIcon(
        "btn-fullscreen",
        force
            ? "assets/icons/normal-straight/compress.svg"
            : "assets/icons/normal-straight/expand.svg",
    );
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
