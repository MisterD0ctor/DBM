import { setButtonIcon } from "../utils/setButtonIcon.js";

export function setOverlay(isShown) {
    document.getElementById("controls-panel")?.classList.toggle("hidden", !isShown);
    document.getElementById("player")?.classList.toggle("hidden", !isShown);
}

export function setAmbient(isAmbient) {
    setButtonIcon(
        "btn-ambient",
        isAmbient
            ? "assets/icons/normal-straight/lightbulb-slash.svg"
            : "assets/icons/normal-straight/bulb.svg",
    );
}

export function setPanscan(isPanscan) {
    setButtonIcon(
        "btn-panscan",
        isPanscan
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

export function setFullscreen(isFullscreen) {
    setButtonIcon(
        "btn-fullscreen",
        isFullscreen
            ? "assets/icons/normal-straight/compress.svg"
            : "assets/icons/normal-straight/expand.svg",
    );
}
