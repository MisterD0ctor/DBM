import * as player from "./player.js";
import * as ui from "./ui/ui.js";

// --- Populate from mpv -------------------------------------------------------

export async function populatePlaylistMenu() {
    const playlist = await player.getPlaylist();
    const pos = await player.getPlaylistPos().catch(() => 0);
    const paused = await player.getPause().catch(() => false);

    // Fetch saved watch-later positions for all playlist entries
    const paths = playlist.map((e) => e.filename ?? "").filter(Boolean);
    const savedPositions = await player.getWatchLaterPositions(paths).catch(() => ({}));

    ui.populatePlaylistMenu(playlist, pos, paused, savedPositions, (index) => {
        player.playlistPlayIndex(index).then(() => setTimeout(() => player.play(), 100));
    });
}

// --- Menu toggle + click-outside-to-close ------------------------------------

const playlistMenu = document.getElementById("playlist-menu");
const btnPlaylist = document.getElementById("btn-playlist");

btnPlaylist.onclick = () => ui.togglePlaylistMenu();

document.addEventListener("click", (event) => {
    if (!playlistMenu.contains(event.target) && !btnPlaylist.contains(event.target)) {
        ui.togglePlaylistMenu(false);
    }
});
