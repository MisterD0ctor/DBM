import * as player from "./player.js";
import * as ui from "./ui/ui.js";

let duration = 0;

export function setDuration(d) {
    duration = d;
}

// --- Scrubbing ---------------------------------------------------------------

const seekTrack = document.getElementById("seek-track");
let scrubbing = false;
let wasPlayingBeforeScrub = false;

function scrubTo(clientX) {
    const rect = seekTrack.getBoundingClientRect();
    const fraction = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const percent = fraction * 100;
    player.seek(percent, "absolute-percent", "exact");
    ui.setSeekTimeHighlight(true, fraction * duration, percent);
}

seekTrack.addEventListener("mousedown", (event) => {
    if (event.button !== 0) return;
    scrubbing = true;
    wasPlayingBeforeScrub = false;
    player.getProperty("pause", "flag").then((paused) => {
        wasPlayingBeforeScrub = !paused;
        if (wasPlayingBeforeScrub) player.pause();
    });
    scrubTo(event.clientX);
});

document.addEventListener("mousemove", (event) => {
    if (scrubbing) {
        scrubTo(event.clientX);
    } else if (seekTrack.matches(":hover")) {
        const rect = seekTrack.getBoundingClientRect();
        const fraction = (event.clientX - rect.left) / rect.width;
        ui.setSeekTimeHighlight(true, fraction * duration, fraction * 100);
    }
});

document.addEventListener("mouseup", (event) => {
    if (!scrubbing || event.button !== 0) return;
    scrubbing = false;
    if (wasPlayingBeforeScrub) player.play();
    if (!seekTrack.matches(":hover")) ui.setSeekTimeHighlight(false);
});

seekTrack.addEventListener("mouseleave", () => {
    if (!scrubbing) ui.setSeekTimeHighlight(false);
});

document.getElementById("player").addEventListener("mousemove", () => {
    if (!scrubbing && !seekTrack.matches(":hover")) ui.setSeekTimeHighlight(false);
});
