function setButtonIcon(buttonId, src) {
    document.getElementById(buttonId)?.querySelector("img")?.setAttribute("src", src);
}

export function setOverlay(isShown) {
    document.getElementById("controls-panel")?.classList.toggle("hidden", !isShown);
    document.getElementById("player")?.classList.toggle("hidden", !isShown);
}

export function setAmbient(isAmbient) {
    setButtonIcon(
        "btn-ambient",
        isAmbient ? "assets/icons/lightbulb-slash.svg" : "assets/icons/bulb.svg",
    );
}

export function setPanscan(isPanscan) {
    setButtonIcon(
        "btn-panscan",
        isPanscan ? "assets/icons/compress-alt.svg" : "assets/icons/expand-arrows-alt.svg",
    );
}

export function setMute(isMuted) {
    setButtonIcon(
        "btn-mute",
        isMuted ? "assets/icons/volume-mute.svg" : "assets/icons/volume.svg",
    );
}

export function setVolume(volume) {
    document.getElementById("volume-slider").value = volume;
    document.getElementById("volume-display").innerText = volume;
}

export function setFullscreen(isFullscreen) {
    setButtonIcon(
        "btn-fullscreen",
        isFullscreen ? "assets/icons/compress.svg" : "assets/icons/expand.svg",
    );
}
