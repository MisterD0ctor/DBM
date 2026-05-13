import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import { rewind, playNext } from "./navigation.js";

// When the current file ends, mpv emits eof-reached. The play button becomes a
// "restart" button, ArrowRight advances to the next file (or rewinds on the
// last file), and a centered popup gives the user a click target.

let atEnd = false;
let playlistPos = 0;
let playlistCount = 0;

const isLastVideo = () => playlistCount > 0 && playlistPos >= playlistCount - 1;

export function isAtEnd() {
    return atEnd;
}

function exit() {
    atEnd = false;
    ui.setEndOfPlayback(false);
}

/** Used by ArrowRight / popup click — next file, or rewind if it's the last. */
export function advance() {
    exit();
    if (isLastVideo()) rewind();
    else playNext();
}

/** Used by Space / play button — restart the current file. */
export function restart() {
    exit();
    rewind();
}

player.onPropertyChange(({ name, data }) => {
    if (name === "eof-reached") atEnd = !!data;
    else if (name === "playlist-pos") playlistPos = data ?? 0;
    else if (name === "playlist-count") playlistCount = data ?? 0;
    else if (name === "pause" && data === false && atEnd) exit();
});

player.onEvent((event) => {
    if (event.event === "file-loaded" && atEnd) exit();
});

document.getElementById("end-of-playback")?.addEventListener("click", advance);
