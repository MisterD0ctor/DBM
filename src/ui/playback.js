import { formatTime } from "../utils/formatTime.js";
import { parseTvShow } from "../utils/parse.js";
import { setButtonIcon, setButtonTooltip } from "../utils/setButtonIcon.js";
import * as preview from "../preview.js";

let duration;

export function setDuration(seconds) {
    document.getElementById("time-total").textContent = formatTime(seconds);
    duration = seconds;

    const play = document.getElementById("btn-play");
    play.classList.toggle("disabled", seconds === 0);
}

export function setCurrentTime(seconds) {
    document.getElementById("time-current").textContent = formatTime(seconds);
}

export function setProgress(percent) {
    const seekTrack = document.getElementById("seek-track");
    const rect = seekTrack.getBoundingClientRect();
    const pixels = (rect.width * percent) / 100;
    document.documentElement.style.setProperty("--progress", `${pixels}px`);
}

export function setMediaTitle(filename) {
    filename = filename ?? "";

    const playlistBtn = document.getElementById("btn-playlist");
    playlistBtn.classList.toggle("hidden", filename === "");

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

    mediaTitleEl.classList.toggle("overflowing", mediaTitleEl.scrollWidth > mediaTitleEl.clientWidth);
}

export function updateMediaTitleOverflow() {
    const el = document.querySelector(".media-title");
    if (el) el.classList.toggle("overflowing", el.scrollWidth > el.clientWidth);
}

export function setPause(isPaused) {
    setButtonIcon("btn-play", isPaused ? "assets/icons/play.svg" : "assets/icons/pause.svg");
    setButtonTooltip("btn-play", isPaused ? "Play" : "Pause");
}

export function showPlaybackOverlay(action, position) {
    if (position !== undefined) {
        document.documentElement.style.setProperty("--playback-overlay-position", `${position}%`);
    } else {
        document.documentElement.style.setProperty("--playback-overlay-position", `50%`);
    }

    const overlay = document.getElementById("playback-overlay");
    const icon = document.getElementById("playback-status-icon");

    // prettier-ignore
    switch (action) {
        case "pause-on":      icon.src = "assets/icons/pause.svg";                         break;
        case "pause-off":     icon.src = "assets/icons/play.svg";                          break;
        case "seek-backward": icon.src = "assets/icons/seek-backward.svg"; break;
        case "seek-forward":  icon.src = "assets/icons/seek-forward.svg";  break;
        case "rewind":        icon.src = "assets/icons/rotate-left.svg";   break;
        case "previous":      icon.src = "assets/icons/step-backward.svg"; break;
        case "next":          icon.src = "assets/icons/step-forward.svg";  break;
        case "autoplay-on":   icon.src = "assets/icons/arrow-right.svg";   break;
        case "autoplay-off":  icon.src = "assets/icons/arrow-right-slash.svg"; break;
        case "panscan-on":    icon.src = "assets/icons/expand-alt.svg"; break;
        case "panscan-off":   icon.src = "assets/icons/compress-alt.svg"; break;
        case "fullscreen-on": icon.src = "assets/icons/expand.svg"; break;
        case "fullscreen-off":icon.src = "assets/icons/compress.svg"; break;
        case "ambient-on":    icon.src = "assets/icons/lightbulb.svg"; break;
        case "ambient-off":   icon.src = "assets/icons/lightbulb-slash.svg"; break;
        case "mute-on":       icon.src = "assets/icons/volume-mute.svg"; break;
        case "mute-off":      icon.src = "assets/icons/volume.svg"; break;
        case "subtitles-on":  icon.src = "assets/icons/subtitles.svg"; break;
        case "subtitles-off": icon.src = "assets/icons/subtitles-slash.svg"; break;
        default: return;
    }

    overlay.classList.remove("visible");
    void overlay.offsetWidth;
    overlay.classList.add("visible");
}

let seekTimeTooltipTimes = [];
let seekTimeHighlightTimes = [];

export function setSeekTooltip(isShown, clientX) {
    const timeEl = document.getElementById("seek-tooltip");
    const previewEl = document.getElementById("seek-preview");
    timeEl?.classList.toggle("hidden", !isShown);
    previewEl?.classList.toggle("hidden", !isShown);

    if (isShown) {
        seekTimeTooltipTimes.forEach((id) => clearTimeout(id));
        seekTimeTooltipTimes = [];

        const seekTrack = document.getElementById("seek-track");
        const rect = seekTrack.getBoundingClientRect();
        const rectX = Math.max(0, clientX - rect.left);
        const fraction = rectX / rect.width ?? 0;
        const timeSeconds = duration * fraction;

        preview.showAtFraction(fraction);

        const secondsPerPixel = duration / rect.width ?? 0;
        const fractionDigits = Math.max(0, Math.min(4, -Math.round(Math.log10(secondsPerPixel)) ?? 0));

        timeEl.querySelector(".tooltip-text").textContent = formatTime(timeSeconds, fractionDigits);
        document.documentElement.style.setProperty("--seek-tooltip-pos", `${rectX}px`);
    } else {
        seekTimeTooltipTimes.push(
            setTimeout(() => document.documentElement.style.setProperty("--seek-tooltip-pos", `${0}px`), 200),
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
