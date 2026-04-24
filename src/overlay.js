import * as ui from "./ui/ui.js";

const OVERLAY_HIDE_DELAY_MS = 3000;

let hideTimer = null;

function scheduleHide() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
        // Don't hide while the cursor is parked over an open menu — the user
        // is clearly interacting with it even without moving the mouse.
        if (document.querySelector(".menu:not(.hidden):hover")) {
            scheduleHide();
            return;
        }
        ui.toggleOverlay(false);
    }, OVERLAY_HIDE_DELAY_MS);
}

export function showOverlay() {
    ui.toggleOverlay(true);
    scheduleHide();
}

const player = document.getElementById("player");
player.addEventListener("mousemove", showOverlay);
player.addEventListener("mouseenter", showOverlay);
player.addEventListener("mousedown", showOverlay);
player.addEventListener("wheel", showOverlay, { passive: true });
player.addEventListener("mouseleave", () => ui.toggleOverlay(false));
