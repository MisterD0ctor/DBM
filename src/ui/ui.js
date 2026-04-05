import { formatTime } from "../utils/formatTime.js";
import { partition } from "../utils/partition.js";

// ─── Playback state ───────────────────────────────────────────────────────────

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
    const graphic = document.querySelector(".play-button-graphic");
    const before = document.querySelector(".play-button-before");
    const after = document.querySelector(".play-button-after");
    graphic?.classList.toggle("paused", isPaused);
    before?.classList.toggle("paused", isPaused);
    after?.classList.toggle("paused", isPaused);
}

// ─── Controls visibility ──────────────────────────────────────────────────────

export function setOverlay(isShown) {
    document.getElementById("controls")?.classList.toggle("hidden", !isShown);
    document.getElementById("overlay")?.classList.toggle("hidden", !isShown);
}

// ─── Seek highlight ───────────────────────────────────────────────────────────

export function setSeekTimeHighlight(isShown, seconds, percentPos) {
    const timeEl = document.getElementById("seek-time-highlight");
    const highlightEl = document.querySelector(".seek .highlight");
    timeEl?.classList.toggle("hidden", !isShown);
    highlightEl?.classList.toggle("hidden", !isShown);
    if (seconds !== undefined) {
        timeEl.textContent = formatTime(seconds);
    }
    if (percentPos !== undefined) {
        document.documentElement.style.setProperty("--seek", `${percentPos}%`);
    }
}

// ─── Button icon helpers ──────────────────────────────────────────────────────

function setButtonIcon(buttonId, src) {
    document.getElementById(buttonId)?.querySelector("img")?.setAttribute("src", src);
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

// ─── Ambient lighting ─────────────────────────────────────────────────────────

/**
 * Show or hide the ambient overlay and toggle the button icon.
 * When hiding, also clear any colors so bars fade out cleanly.
 */
export function setAmbientOverlay(isEnabled) {
    // document.getElementById("ambient-overlay")?.classList.toggle("hidden", !isEnabled);
    setAmbient(isEnabled);
}

export function getAspectRatio() {
    const container = document.getElementById("overlay");
    if (!container) return 1;
    return container.clientWidth / container.clientHeight;
}

export function setAmbientAspectRatio(videoAspect) {
    const containerAspect = getAspectRatio();

    if (videoAspect > containerAspect) {
        // Wider than container — pillarbox ambient bars on top and bottom
        document
            .getElementById("ambient-top")
            .style.setProperty("--height", `${((1 - containerAspect / videoAspect) / 2) * 100.1}%`);
        document
            .getElementById("ambient-bottom")
            .style.setProperty("--height", `${((1 - containerAspect / videoAspect) / 2) * 100.1}%`);
        document.getElementById("ambient-left").style.setProperty("--width", "0px");
        document.getElementById("ambient-right").style.setProperty("--width", "0px");
    } else {
        // Taller than container — letterbox ambient bars on left and right
        document.getElementById("ambient-top").style.setProperty("--height", "0px");
        document.getElementById("ambient-bottom").style.setProperty("--height", "0px");
        document
            .getElementById("ambient-left")
            .style.setProperty("--width", `${((1 - videoAspect / containerAspect) / 2) * 100.1}%`);
        document
            .getElementById("ambient-right")
            .style.setProperty("--width", `${((1 - videoAspect / containerAspect) / 2) * 100.1}%`);
    }
}

// ─── Tracks menus ─────────────────────────────────────────────────────────────

export function toggleTrackListMenu() {
    document.getElementById("track-list-menu")?.classList.toggle("hidden");
}

export function hideTracksMenu() {
    document.getElementById("track-list-menu")?.classList.add("hidden");
}

export function showTracksMenu() {
    document.getElementById("track-list-menu")?.classList.remove("hidden");
}

export function populateSubtitleTrackMenu(subtitleTrackList, onSelect, onDisable) {
    const menu = document.getElementById("subtitle-track-menu");
    menu.innerHTML = "";

    const noneItem = document.createElement("div");
    noneItem.id = "no";
    noneItem.className = "item";
    noneItem.textContent = "Off";
    noneItem.onclick = () => {
        onDisable();
        hideTracksMenu();
    };
    menu.appendChild(noneItem);

    populateTrackMenu(menu, subtitleTrackList, onSelect);
}

export function populateAudioTrackMenu(audioTrackList, onSelect) {
    const menu = document.getElementById("audio-track-menu");
    menu.innerHTML = "";

    populateTrackMenu(menu, audioTrackList, onSelect);
}

function populateTrackMenu(menu, trackList, onSelect) {
    const duplicatedLangs = getDuplicatedLanguages(trackList);

    for (const track of trackList) {
        const item = document.createElement("div");
        item.id = track.id;
        item.className = "item";
        item.textContent = getTrackTitle(track, duplicatedLangs.includes(track.lang));
        item.onclick = () => {
            onSelect(track.id);
            hideTracksMenu();
        };
        menu.appendChild(item);
    }

    resizeTrackListMenus();
}

export function setSelectedSubtitleTrack(id) {
    const menu = document.getElementById("subtitle-track-menu");
    setSelectedTrack(menu, id);
}

export function setSelectedAudioTrack(id) {
    const menu = document.getElementById("audio-track-menu");
    setSelectedTrack(menu, id);
}

function setSelectedTrack(menu, id) {
    Array.from(menu.childNodes).forEach((item) => {
        if (item.id == id) {
            item.classList.add("selected");
        } else {
            item.classList.remove("selected");
        }
    });
}

export function resizeTrackListMenus() {
    const container = document.getElementById("track-list-menu");

    const [subMenus, others] = partition(container.childNodes, (element) => {
        return element.classList?.contains("sub-menu");
    });

    // Reset so we can measure natural scroll heights
    subMenus.forEach((m) => {
        m.style.maxHeight = "";
        m.style.minHeight = "";
    });

    const totalOtherHeight = Array.from(others).reduce((sum, element) => {
        return sum + element.scrollHeight;
    }, 0);

    const containerHeight = container.clientHeight;
    const available = containerHeight - totalOtherHeight;

    const share = available / subMenus.length;

    const [uncapped, capped] = partition([...subMenus], (menu) => menu.scrollHeight < share);
    const totalUncappedHeight = uncapped.reduce((sum, menu) => {
        return sum + menu.scrollHeight;
    }, 0);

    const remaining = available - totalUncappedHeight;
    const remainingShare = remaining / capped.length;

    capped.forEach((menu) => (menu.style.maxHeight = `${remainingShare}px`));
    uncapped.forEach((menu) => (menu.style.minHeight = "fit-content"));
}

const observer = new ResizeObserver(() => resizeTrackListMenus());
// observer.observe(document.getElementById("track-list-menu"));

// ─── Utilities ────────────────────────────────────────────────────────────────

function getTrackTitle(track, hasDuplicateLanguages) {
    const language = languageCodeEndonym(track.lang) ?? "";
    const title = track.title ?? "";
    const hasLang = language != "";
    const hasTitle = title != "";
    const titleIncludesLang = track.title?.toLowerCase().includes(language?.toLowerCase());

    if (!hasLang && !hasTitle) return `Track ${track.id}`;
    if (!hasLang && hasTitle) return title;
    if (hasLang && !hasDuplicateLanguages) return language;
    if (hasLang && !hasTitle && hasDuplicateLanguages) return `${language} - Track ${track.id}`;
    if (hasLang && hasTitle && hasDuplicateLanguages && !titleIncludesLang)
        return `${language} - ${title}`;
    if (hasLang && hasTitle && hasDuplicateLanguages && titleIncludesLang) return title;

    if (!hasLang) return hasTitle ? title : `Track ${track.id}`;
    if (!hasTitle || !hasDuplicateLanguages) return language;
    return titleIncludesLang ? title : `${language} - Track ${track.id}`;

    //  hasLang | hasTitle | hasDuplicateLangs | titleIncludesLang | return
    // ---------|----------|-------------------|-------------------|-------
    //     0    |    0     |         x         |         x         | id
    //     0    |    1     |         x         |         x         | title
    //     1    |    0     |         0         |         x         | lang
    //     1    |    0     |         1         |         x         | lang - id
    //     1    |    1     |         0         |         0         | lang
    //     1    |    1     |         0         |         1         | lang
    //     1    |    1     |         1         |         0         | lang - title
    //     1    |    1     |         1         |         1         | title

    //  has lang | has title | duplicate lang | duplicate title | title includes lang | return
    // ----------|-----------|----------------|-----------------|---------------------|-------
    //     0     |     0     |       x        |        x        |          x          | id
    //     0     |     1     |       x        |        0        |          x          | title
    //     0     |     1     |       x        |        1        |          x          | title - id
    //     1     |     0     |       0        |        x        |          x          | lang
    //     1     |     0     |       1        |        x        |          x          | lang - id
    //     1     |     1     |       0        |        0        |          0          | lang
    //     1     |     1     |       0        |        0        |          1          | lang
    //     1     |     1     |       0        |        1        |          0          | lang
    //     1     |     1     |       0        |        1        |          1          | lang
    //     1     |     1     |       1        |        0        |          0          | lang - title
    //     1     |     1     |       1        |        0        |          1          | title
    //     1     |     1     |       1        |        1        |          0          | lang - title
    //     1     |     1     |       1        |        1        |          1          | title
}

function getDuplicatedLanguages(tracks) {
    const languageOccurrences = tracks
        .map((track) => track.lang)
        .filter((lang) => lang !== undefined)
        .reduce((count = {}, lang) => {
            if (count[lang] !== undefined) {
                count[lang]++;
            } else {
                count[lang] = 1;
            }
            return count;
        }, {});

    return Object.keys(languageOccurrences).filter((lang) => languageOccurrences[lang] > 1);
}

function languageCodeEndonym(code) {
    if (code == undefined || code == "") return undefined;
    try {
        const locale = new Intl.Locale(code);
        const lang = locale.language;
        const name = new Intl.DisplayNames([lang], { type: "language" }).of(lang);
        if (!name) return code;
        return name[0].toLocaleUpperCase(lang) + name.slice(1);
    } catch {
        return code;
    }
}
