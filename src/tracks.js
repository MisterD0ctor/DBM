import * as player from "./player.js";
import * as ui from "./ui/ui.js";

// --- Populate from mpv -------------------------------------------------------

export async function populateTrackListMenu() {
    const trackList = await player.getTrackList();
    const subtitle = trackList.filter((t) => t.type === "sub");
    const audio = trackList.filter((t) => t.type === "audio");

    const activeSubtitleId = subtitle.find((t) => t.selected)?.id ?? "no";
    const activeAudioId = audio.find((t) => t.selected)?.id;

    ui.populateSubtitleTrackMenu(
        subtitle,
        (id) => player.setSid(id.toString()).then(() => player.setSubVisibility(true)),
        () => player.setSid("no"),
    );

    ui.populateAudioTrackMenu(audio, (id) => {
        player.setAid(id.toString());
    });

    ui.setActiveSubtitleTrack(activeSubtitleId);
    ui.setActiveAudioTrack(activeAudioId);
}

// --- Menu toggle + click-outside-to-close ------------------------------------

const tracksMenu = document.getElementById("tracks-menu");
const btnTracks = document.getElementById("btn-tracks");

btnTracks.onclick = () => ui.toggleTrackListMenu();

document.addEventListener("click", (event) => {
    if (!tracksMenu.contains(event.target) && !btnTracks.contains(event.target)) {
        ui.toggleTrackListMenu(false);
    }
});

// --- Observe window size changes ---------------------------------------------

const observer = new ResizeObserver(() => ui.resizeTrackListMenus());
observer.observe(document.querySelector("body"));
