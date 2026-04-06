import * as ui from "./ui/ui.js";

const OVERLAY_HIDE_DELAY_MS = 3000;

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
