import { formatTime } from "../utils/formatTime.js";

export function setDuration(seconds) {
    document.getElementById("total-time").textContent = formatTime(seconds);
}

export function setCurrentTime(seconds) {
    document.getElementById("current-time").textContent = formatTime(seconds);
}

export function setProgress(percent) {
    document.documentElement.style.setProperty("--progress", `${percent}%`);
}

export function setFilename(filename) {
    const el = document.querySelector(".filename");
    el.title = filename ?? "";
    if (filename) {
        const dot = filename.lastIndexOf(".");
        filename = dot > 0 ? filename.substring(0, dot) : filename;
    }
    el.textContent = filename ?? "";
}

export function setPause(isPaused) {
    const before = document.querySelector(".play-button-before");
    const after = document.querySelector(".play-button-after");
    before?.classList.toggle("paused", isPaused);
    after?.classList.toggle("paused", isPaused);
}

export function setLoaded(isLoaded) {
    document.querySelector("body")?.classList.toggle("loaded", isLoaded);
    document.getElementById("open-path-button")?.classList.toggle("loaded", isLoaded);
}

export function setSeekTimeHighlight(isShown, timeSeconds, percentPos) {
    const timeEl = document.getElementById("seek-time-highlight");
    const highlightEl = document.querySelector(".seek .highlight");
    timeEl?.classList.toggle("hidden", !isShown);
    highlightEl?.classList.toggle("hidden", !isShown);
    if (timeSeconds !== undefined) {
        timeEl.textContent = formatTime(timeSeconds);
    }
    if (percentPos !== undefined) {
        document.documentElement.style.setProperty("--seek", `${percentPos}%`);
    }
}
