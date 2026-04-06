export function getAspectRatio() {
    const container = document.getElementById("overlay");
    if (!container) return 1;
    return container.clientWidth / container.clientHeight;
}

export function setAmbientOverlay(isEnabled) {
    // Importing here would create a circular dep; inline the icon toggle.
    const img = document.getElementById("ambient-button")?.querySelector("img");
    img?.setAttribute(
        "src",
        isEnabled ? "assets/icons/lightbulb-slash.svg" : "assets/icons/bulb.svg",
    );
}

export function setAmbientAspectRatio(videoAspect) {
    const containerAspect = getAspectRatio();

    if (videoAspect > containerAspect) {
        const barSize = `${((1 - containerAspect / videoAspect) / 2) * 100.1}%`;
        document.getElementById("ambient-top").style.setProperty("--height", barSize);
        document.getElementById("ambient-bottom").style.setProperty("--height", barSize);
        document.getElementById("ambient-left").style.setProperty("--width", "0px");
        document.getElementById("ambient-right").style.setProperty("--width", "0px");
    } else {
        const barSize = `${((1 - videoAspect / containerAspect) / 2) * 100.1}%`;
        document.getElementById("ambient-top").style.setProperty("--height", "0px");
        document.getElementById("ambient-bottom").style.setProperty("--height", "0px");
        document.getElementById("ambient-left").style.setProperty("--width", barSize);
        document.getElementById("ambient-right").style.setProperty("--width", barSize);
    }
}
