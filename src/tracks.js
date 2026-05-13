import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import { closeOnOutsideClick } from "./utils/closeOnOutsideClick.js";

// --- Populate from mpv -------------------------------------------------------

export async function populateTrackListMenu() {
    const trackList = await player.getTrackList();
    const subtitle = trackList.filter((t) => t.type === "sub");
    const audio = trackList.filter((t) => t.type === "audio");

    const activeSubtitleId = (await player.getSubVisibility())
        ? (subtitle.find((t) => t.selected)?.id ?? "no")
        : "no";
    const activeAudioId = audio.find((t) => t.selected)?.id;

    ui.populateSubtitleTrackMenu(
        subtitle,
        (id) => player.setSid(id.toString()).then(() => player.setSubVisibility(true)),
        () => player.setSubVisibility(false),
        () => player.openSubtitleDialog(),
    );

    ui.populateAudioTrackMenu(audio, (id) => {
        player.setAid(id.toString());
    });

    ui.setActiveSubtitleTrackID(activeSubtitleId);
    ui.setActiveAudioTrackID(activeAudioId);
}

// --- Menu toggle + click-outside-to-close ------------------------------------

const tracksMenu = document.getElementById("tracks-menu");
const btnTracks = document.getElementById("btn-tracks");

btnTracks.onclick = () => ui.toggleTrackListMenu();

closeOnOutsideClick(tracksMenu, btnTracks, () => ui.toggleTrackListMenu(false));

// --- Observe window size changes ---------------------------------------------

const observer = new ResizeObserver(() => ui.resizeTrackListMenus());
observer.observe(document.querySelector("body"));
