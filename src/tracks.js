import * as player from "./player.js";
import * as ui from "./ui/ui.js";

// --- Populate from mpv -------------------------------------------------------

export async function populateTrackListMenu() {
    const trackListValue = await player.getProperty("track-list", "string");
    const trackList =
        typeof trackListValue === "string" ? JSON.parse(trackListValue) : trackListValue;
    const subtitle = trackList.filter((t) => t.type === "sub");
    const audio = trackList.filter((t) => t.type === "audio");

    console.log(subtitle);

    const activeSubtitleId = subtitle.find((t) => t.selected)?.id ?? "no";
    const activeAudioId = audio.find((t) => t.selected)?.id;

    ui.populateSubtitleTrackMenu(
        subtitle,
        (id) =>
            player
                .setProperty("sid", id.toString())
                .then(() => player.setProperty("sub-visibility", "yes")),
        () => player.setProperty("sid", "no"),
    );

    ui.populateAudioTrackMenu(audio, (id) => {
        player.setProperty("aid", id.toString());
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
