import { formatTime } from "../utils/formatTime.js";

export function setDuration(seconds) {
    document.getElementById("time-total").textContent = formatTime(seconds);
}

export function setCurrentTime(seconds) {
    document.getElementById("time-current").textContent = formatTime(seconds);
}

export function setProgress(percent) {
    document.documentElement.style.setProperty("--progress", `${percent}%`);
}

export function setFilename(filename) {
    const el = document.querySelector(".media-title");
    el.title = filename ?? "";
    if (filename) {
        const dot = filename.lastIndexOf(".");
        filename = dot > 0 ? filename.substring(0, dot) : filename;
    }
    el.textContent = filename ?? "";
}

export function setPause(isPaused) {
    const before = document.querySelector(".play-icon-left");
    const after = document.querySelector(".play-icon-right");
    before?.classList.toggle("paused", isPaused);
    after?.classList.toggle("paused", isPaused);
}

export function setLoaded(isLoaded) {
    // document.querySelector("body")?.classList.toggle("loaded", isLoaded);
    document.getElementById("btn-open")?.classList.toggle("loaded", isLoaded);
}

export function setSeekTimeHighlight(isShown, timeSeconds, percentPos) {
    const timeEl = document.getElementById("seek-tooltip");
    const highlightEl = document.querySelectorAll(".seek-highlight");
    timeEl?.classList.toggle("hidden", !isShown);
    highlightEl.forEach((el) => el?.classList.toggle("hidden", !isShown));
    if (timeSeconds !== undefined) {
        timeEl.textContent = formatTime(timeSeconds);
    }
    if (percentPos !== undefined) {
        document.documentElement.style.setProperty("--seek", `${percentPos}%`);
    }
}
