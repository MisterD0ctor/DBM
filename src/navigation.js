import * as player from "./player.js";

// When the user explicitly advances (next/prev), they want the new file to
// start fresh — override mpv's watch-later resume position by seeking to 0
// once the file is loaded.
let resetOnNextLoad = false;

player.onEvent((event) => {
    if (event.event === "file-loaded" && resetOnNextLoad) {
        resetOnNextLoad = false;
        player.seek(0, "absolute");
    }
});

export function rewind() {
    player.seek(0, "absolute");
    player.play();
}

export function playPrevious() {
    resetOnNextLoad = true;
    player.playlistPrev().then(() => setTimeout(() => player.play(), 100));
}

export function playNext() {
    resetOnNextLoad = true;
    player.playlistNext().then(() => setTimeout(() => player.play(), 100));
}
