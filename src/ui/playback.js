import { formatTime } from "../utils/formatTime.js";

let duration;

export function setDuration(seconds) {
    document.getElementById("time-total").textContent = formatTime(seconds);
    duration = seconds;

    const play = document.getElementById("btn-play");
    play.classList.toggle("disabled", seconds === 0);
}

export function setCurrentTime(seconds) {
    document.getElementById("time-current").textContent = formatTime(seconds);

    const seekBack = document.getElementById("btn-seek-back");
    const seekForward = document.getElementById("btn-seek-forward");

    const margin = 0.5;

    seekBack.classList.toggle("disabled", seconds < margin);
    seekForward.classList.toggle("disabled", seconds > duration - margin);

    seekBack.disabled = seconds < margin;
    seekForward.disabled = seconds > duration - margin;
}

export function setProgress(percent) {
    const seekTrack = document.getElementById("seek-track");
    const rect = seekTrack.getBoundingClientRect();
    const pixels = (rect.width * percent) / 100;
    document.documentElement.style.setProperty("--progress", `${pixels}px`);
}

export function setMediaTitle(mediaTitle) {
    mediaTitle = mediaTitle ?? "";
    const el = document.querySelector(".media-title");
    el.title = mediaTitle;
    if (mediaTitle) {
        const dot = mediaTitle.lastIndexOf(".");
        mediaTitle = dot > 0 ? mediaTitle.substring(0, dot) : mediaTitle;
    }
    el.textContent = mediaTitle;
    const isLoaded = mediaTitle !== "";
    document.getElementById("btn-open")?.classList.toggle("loaded", isLoaded);
}

export function setPause(isPaused) {
    const before = document.querySelector(".play-icon-left");
    const after = document.querySelector(".play-icon-right");
    before?.classList.toggle("paused", isPaused);
    after?.classList.toggle("paused", isPaused);
}

let seekTimeTooltipTimes = [];
let seekTimeHighlightTimes = [];

export function setSeekTooltip(isShown, clientX) {
    const timeEl = document.getElementById("seek-tooltip");
    timeEl?.classList.toggle("hidden", !isShown);

    if (isShown) {
        seekTimeTooltipTimes.forEach((id) => clearTimeout(id));
        seekTimeTooltipTimes = [];

        const seekTrack = document.getElementById("seek-track");
        const rect = seekTrack.getBoundingClientRect();
        const rectX = clientX - rect.left;
        const fraction = Math.max(0, rectX / rect.width ?? 0);
        const timeSeconds = duration * fraction;

        const secondsPerPixel = duration / rect.width ?? 0;
        const fractionDigits = Math.max(
            0,
            Math.min(4, -Math.round(Math.log10(secondsPerPixel)) ?? 0),
        );

        timeEl.textContent = formatTime(timeSeconds, fractionDigits);
        document.documentElement.style.setProperty("--seek-tooltip-pos", `${rectX}px`);
    } else {
        seekTimeTooltipTimes.push(
            setTimeout(
                () => document.documentElement.style.setProperty("--seek-tooltip-pos", `${0}px`),
                200,
            ),
        );
    }
}

export function setSeekHighlight(isShown, clientX) {
    const highlightElList = document.querySelectorAll(".seek-highlight");
    highlightElList.forEach((el) => el?.classList.toggle("hidden", !isShown));

    if (isShown) {
        seekTimeHighlightTimes.forEach((id) => clearTimeout(id));
        seekTimeHighlightTimes = [];

        const seekTrack = document.getElementById("seek-track");
        const rect = seekTrack.getBoundingClientRect();
        const rectX = clientX - rect.left;

        document.documentElement.style.setProperty("--seek-highlight-pos", `${rectX}px`);
    } else {
        seekTimeHighlightTimes.push(
            setTimeout(
                () => document.documentElement.style.setProperty("--seek-highlight-pos", `${0}px`),
                200,
            ),
        );
    }
}
