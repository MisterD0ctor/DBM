import * as ui from "./ui/ui.js";

const OVERLAY_HIDE_DELAY_MS = 3000;

let hideTimer = null;

function showOverlay() {
    ui.setOverlay(true);
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => ui.setOverlay(false), OVERLAY_HIDE_DELAY_MS);
}

const player = document.getElementById("player");
player.addEventListener("mousemove", showOverlay);
player.addEventListener("mouseenter", showOverlay);
player.addEventListener("mousedown", showOverlay);
player.addEventListener("mouseleave", () => ui.setOverlay(false));
