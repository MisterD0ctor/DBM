function setButtonIcon(buttonId, src) {
    document.getElementById(buttonId)?.querySelector("img")?.setAttribute("src", src);
}

export function setOverlay(isShown) {
    document.getElementById("controls")?.classList.toggle("hidden", !isShown);
    document.getElementById("overlay")?.classList.toggle("hidden", !isShown);
}

export function setAmbient(isAmbient) {
    setButtonIcon(
        "ambient-button",
        isAmbient ? "assets/icons/lightbulb-slash.svg" : "assets/icons/bulb.svg",
    );
}

export function setPanscan(isPanscan) {
    setButtonIcon(
        "panscan-button",
        isPanscan ? "assets/icons/compress-alt.svg" : "assets/icons/expand-arrows-alt.svg",
    );
}

export function setMute(isMuted) {
    setButtonIcon(
        "mute-button",
        isMuted ? "assets/icons/volume-mute.svg" : "assets/icons/volume.svg",
    );
}

export function setVolume(volume) {
    document.getElementById("volume-slider").value = volume;
    document.getElementById("volume-value").innerText = volume;
}

export function setFullscreen(isFullscreen) {
    setButtonIcon(
        "fullscreen-button",
        isFullscreen ? "assets/icons/compress.svg" : "assets/icons/expand.svg",
    );
}
