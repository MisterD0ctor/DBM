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

    const noneItem = createTrackMenuItem("Off", "no", () => {
        onDisable();
        hideTracksMenu();
    });
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
        const title = getTrackTitle(track, duplicatedLangs.includes(track.lang));
        const id = track.id;
        menu.appendChild(createTrackMenuItem(title, id, onSelect));
    }

    resizeTrackListMenus();
}

function createTrackMenuItem(title, id, onSelect) {
    const item = document.createElement("div");
    const selectedHighlight = document.createElement("div");
    selectedHighlight.classList.add("highlight");
    const titleEl = document.createElement("span");
    titleEl.classList.add("title");
    item.id = id;
    item.className = "track-item";
    item.onclick = () => {
        onSelect(id);
        hideTracksMenu();
    };
    titleEl.textContent = title;
    item.appendChild(selectedHighlight);
    item.appendChild(titleEl);
    return item;
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

/// Cap each tracks-list so the menu fits within its max-height.
/// Lists that naturally fit get their full height; overflow is distributed
/// evenly among the remaining lists.
export function resizeTrackListMenus() {
    const container = document.getElementById("tracks-menu");
    const lists = [...container.querySelectorAll(".tracks-list")];
    if (lists.length === 0) return;

    // Reset so we can measure natural (unconstrained) heights
    lists.forEach((m) => (m.style.maxHeight = ""));

    // The max-height from CSS on the container is the hard budget
    const maxHeight = parseFloat(getComputedStyle(container).maxHeight) || window.innerHeight;
    const fixedHeight = [...container.children]
        .filter((el) => !el.classList.contains("tracks-list"))
        .reduce((sum, el) => sum + el.offsetHeight, 0);
    const padding =
        parseFloat(getComputedStyle(container).paddingTop) +
        parseFloat(getComputedStyle(container).paddingBottom);

    let available = maxHeight - fixedHeight - padding;

    // First pass: lists that fit within an equal share keep their natural size
    const share = available / lists.length;
    const small = lists.filter((m) => m.scrollHeight <= share);
    const large = lists.filter((m) => m.scrollHeight > share);

    const smallTotal = small.reduce((sum, m) => sum + m.scrollHeight, 0);
    const remaining = available - smallTotal;
    const cap = large.length > 0 ? remaining / large.length : 0;

    large.forEach((m) => (m.style.maxHeight = `${cap}px`));
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
