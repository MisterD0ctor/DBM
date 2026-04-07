import { partition } from "../utils/partition.js";

// --- Menu visibility ---------------------------------------------------------

export function toggleTrackListMenu() {
    document.getElementById("tracks-menu")?.classList.toggle("hidden");
}

export function hideTracksMenu() {
    document.getElementById("tracks-menu")?.classList.add("hidden");
}

// --- Populate menus ----------------------------------------------------------

export function populateSubtitleTrackMenu(subtitleTrackList, onSelect, onDisable) {
    const menu = document.getElementById("tracks-subtitle");
    menu.innerHTML = "";

    const noneItem = document.createElement("div");
    noneItem.id = "no";
    noneItem.className = "track-item";
    noneItem.textContent = "Off";
    noneItem.onclick = () => {
        onDisable();
        hideTracksMenu();
    };
    menu.appendChild(noneItem);

    populateTrackMenu(menu, subtitleTrackList, onSelect);
}

export function populateAudioTrackMenu(audioTrackList, onSelect) {
    const menu = document.getElementById("tracks-audio");
    menu.innerHTML = "";

    populateTrackMenu(menu, audioTrackList, onSelect);
}

function populateTrackMenu(menu, trackList, onSelect) {
    const duplicatedLangs = getDuplicatedLanguages(trackList);

    for (const track of trackList) {
        const item = document.createElement("div");
        item.id = track.id;
        item.className = "track-item";
        item.textContent = getTrackTitle(track, duplicatedLangs.includes(track.lang));
        item.onclick = () => {
            onSelect(track.id);
            hideTracksMenu();
        };
        menu.appendChild(item);
    }

    resizeTrackListMenus();
}

// --- Selection ---------------------------------------------------------------

export function setSelectedSubtitleTrack(id) {
    const menu = document.getElementById("tracks-subtitle");
    setSelectedTrack(menu, id);
}

export function setSelectedAudioTrack(id) {
    const menu = document.getElementById("tracks-audio");
    setSelectedTrack(menu, id);
}

function setSelectedTrack(menu, id) {
    for (const item of menu.children) {
        item.classList.toggle("selected", item.id == id);
    }
}

// --- Resize ------------------------------------------------------------------

export function resizeTrackListMenus() {
    const container = document.getElementById("tracks-menu");

    const [subMenus, others] = partition(container.childNodes, (element) => {
        return element.classList?.contains("tracks-list");
    });

    subMenus.forEach((m) => {
        m.style.maxHeight = "";
        m.style.minHeight = "";
    });

    const totalOtherHeight = Array.from(others).reduce((sum, el) => sum + el.scrollHeight, 0);
    const available = container.clientHeight - totalOtherHeight;
    const share = available / subMenus.length;

    const [uncapped, capped] = partition([...subMenus], (menu) => menu.scrollHeight < share);
    const totalUncappedHeight = uncapped.reduce((sum, menu) => sum + menu.scrollHeight, 0);
    const remainingShare = (available - totalUncappedHeight) / capped.length;

    capped.forEach((menu) => (menu.style.maxHeight = `${remainingShare}px`));
    uncapped.forEach((menu) => (menu.style.minHeight = "fit-content"));
}

resizeTrackListMenus();
const observer = new ResizeObserver(() => resizeTrackListMenus());
observer.observe(document.getElementById("tracks-menu"));

// --- Utilities ---------------------------------------------------------------

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
}

function getDuplicatedLanguages(tracks) {
    const counts = {};
    for (const track of tracks) {
        if (track.lang !== undefined) {
            counts[track.lang] = (counts[track.lang] || 0) + 1;
        }
    }
    return Object.keys(counts).filter((lang) => counts[lang] > 1);
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
