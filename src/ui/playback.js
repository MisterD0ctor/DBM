import { formatTime } from "../utils/formatTime.js";
import { parseTvShow } from "../utils/parse.js";
import { setButtonIcon } from "../utils/setButtonIcon.js";

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

export function setMediaTitle(filename) {
    filename = filename ?? "";

    const btnEl = document.getElementById("btn-open");
    const mediaTitleEl = document.querySelector(".media-title");
    const titleEl = document.querySelector(".media-title .title");
    const showEl = document.querySelector(".media-title .show");
    const episodeEl = document.querySelector(".media-title .episode");
    const episodeTitleEl = document.querySelector(".media-title .episode-title");

    if (filename) {
        const dot = filename.lastIndexOf(".");
        filename = dot > 0 ? filename.substring(0, dot) : filename;
    }

    const episodeData = parseTvShow(filename);
    if (episodeData != null) {
        const { show, season, episode, episodeEnd, title } = episodeData;
        titleEl.textContent = "";
        titleEl.classList.toggle("hidden", true);
        showEl.textContent = show;
        showEl.classList.toggle("hidden", false);
        episodeEl.textContent = `S${season}:E${episode}`;
        episodeEl.classList.toggle("hidden", false);
        if (title != undefined) {
            episodeTitleEl.textContent = title;
            episodeTitleEl.classList.toggle("hidden", false);
        } else {
            episodeTitleEl.textContent = "";
            episodeTitleEl.classList.toggle("hidden", true);
        }
    } else {
        titleEl.textContent = filename;
        titleEl.classList.toggle("hidden", false);
        showEl.textContent = "";
        showEl.classList.toggle("hidden", true);
        episodeEl.textContent = "";
        episodeEl.classList.toggle("hidden", true);
        episodeTitleEl.textContent = "";
        episodeTitleEl.classList.toggle("hidden", true);
    }

    mediaTitleEl.classList.toggle(
        "overflowing",
        mediaTitleEl.scrollWidth > mediaTitleEl.clientWidth,
    );
    const isLoaded = filename !== "";
    btnEl.classList.toggle("loaded", isLoaded);
}

export function updateMediaTitleOverflow() {
    const el = document.querySelector(".media-title");
    if (el) el.classList.toggle("overflowing", el.scrollWidth > el.clientWidth);
}

export function setPause(isPaused) {
    setButtonIcon("btn-play", isPaused ? "assets/icons/play.svg" : "assets/icons/pause.svg");
    showPlaybackOverlay(isPaused);
}

function showPlaybackOverlay(isPaused) {
    const overlay = document.getElementById("playback-overlay");
    const icon = document.getElementById("playback-status-icon");
    icon.src = isPaused ? "assets/icons/pause.svg" : "assets/icons/play.svg";
    overlay.classList.remove("visible");
    void overlay.offsetWidth;
    overlay.classList.add("visible");
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
        const rectX = Math.max(0, clientX - rect.left);
        const fraction = rectX / rect.width ?? 0;
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

export function setSeekHighlight(enable, clientX) {
    const seekTrack = document.getElementById("seek-track");

    seekTrack.classList.toggle("highlighted", enable);

    if (enable) {
        seekTimeHighlightTimes.forEach((id) => clearTimeout(id));
        seekTimeHighlightTimes = [];

        const rect = seekTrack.getBoundingClientRect();
        const rectX = Math.max(0, clientX - rect.left);

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
