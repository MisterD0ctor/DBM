// --- Menu visibility ---------------------------------------------------------

export function toggleTrackListMenu(force) {
    const menu = document.getElementById("tracks-menu");
    if (force !== undefined) {
        menu.classList.toggle("hidden", !force);
    } else {
        menu.classList.toggle("hidden");
    }
}

// --- Populate menus ----------------------------------------------------------

export function populateSubtitleTrackMenu(subtitleTrackList, onSelect, onDisable) {
    const menu = document.getElementById("tracks-subtitle");
    menu.innerHTML = "";

    const noneItem = createTrackMenuItem("Off", "no", () => {
        onDisable();
        toggleTrackListMenu(false);
    });
    // const noneImg = document.createElement("img");
    // noneImg.setAttribute("src", "assets/icons/subtitles-slash.svg");
    // noneItem.appendChild(noneImg);
    menu.appendChild(noneItem);

    populateTrackMenu(menu, subtitleTrackList, onSelect);
}

export function populateAudioTrackMenu(audioTrackList, onSelect) {
    const menu = document.getElementById("tracks-audio");
    menu.innerHTML = "";

    populateTrackMenu(menu, audioTrackList, onSelect);
}

function populateTrackMenu(menu, trackList, onSelect) {
    const sorted = sortByLanguage(trackList);
    const titles = buildTrackTitles(sorted);

    for (const track of sorted) {
        menu.appendChild(createTrackMenuItem(titles.get(track), track.id, onSelect));
    }

    resizeTrackListMenus();
}

function createTrackMenuItem(title, id, onSelect) {
    const item = document.createElement("div");
    const activeHighlight = document.createElement("div");
    activeHighlight.classList.add("highlight");
    const titleEl = document.createElement("span");
    titleEl.classList.add("title");
    item.id = id;
    item.className = "menu-item";
    item.onclick = () => {
        onSelect(id);
        toggleTrackListMenu(false);
    };
    titleEl.textContent = title;
    item.appendChild(activeHighlight);
    item.appendChild(titleEl);
    return item;
}

// --- Selection ---------------------------------------------------------------

export function setActiveSubtitleTrack(id) {
    const menu = document.getElementById("tracks-subtitle");
    setActiveTrack(menu, id);
}

export function setActiveAudioTrack(id) {
    const menu = document.getElementById("tracks-audio");
    setActiveTrack(menu, id);
}

function setActiveTrack(menu, id) {
    for (const item of menu.children) {
        item.classList.toggle("active", item.id == id);
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
    lists.forEach((m) => {
        m.style.minHeight = "";
    });

    // getComputedStyle may return the unresolved "min(...)" expression for
    // max-height, so fall back to the browser-resolved rendered height.
    let maxHeight = parseFloat(getComputedStyle(container).maxHeight);
    if (!isFinite(maxHeight)) {
        maxHeight = container.getBoundingClientRect().height || window.innerHeight;
    }
    const fixedHeight = [...container.children]
        .filter((el) => !el.classList.contains("tracks-list"))
        .reduce((sum, el) => sum + getAbsoluteHeight(el), 0);
    const padding =
        parseFloat(getComputedStyle(container).paddingTop) +
        parseFloat(getComputedStyle(container).paddingBottom);

    let available = maxHeight - fixedHeight - padding;

    // First pass: lists that fit within an equal share keep their natural size
    const share = available / lists.length;
    const small = lists.filter((m) => m.scrollHeight <= share);

    small.forEach((m) => (m.style.minHeight = "fit-content"));
}

// --- Utilities ---------------------------------------------------------------

function getAbsoluteHeight(el) {
    const styles = window.getComputedStyle(el);
    const margin = parseFloat(styles.marginTop) + parseFloat(styles.marginBottom);
    return Math.ceil(el.offsetHeight + margin);
}

/// Sort tracks so those sharing a language code are grouped together,
/// preserving the original order within each group and for ungrouped tracks.
function sortByLanguage(trackList) {
    // Build groups keyed by lang, preserving insertion order
    const groups = new Map();
    const noLang = [];
    for (const track of trackList) {
        if (track.lang) {
            if (!groups.has(track.lang)) groups.set(track.lang, []);
            groups.get(track.lang).push(track);
        } else {
            noLang.push(track);
        }
    }
    // Flatten: grouped tracks first (in order of first appearance), then ungrouped
    return [...groups.values()].flat().concat(noLang);
}

/// Build a display title for every track. Tracks sharing the same language
/// get "Language - Title" or "Language - N" labels; unique-language tracks
/// just show the language name; tracks with no language show their title or id.
function buildTrackTitles(trackList) {
    // Count occurrences of each language
    const langCounts = {};
    for (const t of trackList) {
        if (t.lang) langCounts[t.lang] = (langCounts[t.lang] || 0) + 1;
    }

    // Detect base languages (e.g. "es") that have multiple regional variants
    // (e.g. "es-419", "es-ES") so we can show the region qualifier only then.
    const baseLangCodes = {};
    for (const t of trackList) {
        if (!t.lang) continue;
        try {
            const base = new Intl.Locale(t.lang).language;
            if (!baseLangCodes[base]) baseLangCodes[base] = new Set();
            baseLangCodes[base].add(t.lang);
        } catch {
            /* ignore */
        }
    }
    const needsRegion = new Set();
    for (const codes of Object.values(baseLangCodes)) {
        if (codes.size > 1) codes.forEach((c) => needsRegion.add(c));
    }

    // For numbering: track how many of each (lang, title) pair we've seen
    const seen = {};
    // Pre-count (lang, title) pairs to know if numbering is needed
    const pairCounts = {};
    for (const t of trackList) {
        if (!t.lang) continue;
        const key = `${t.lang}\0${t.title ?? ""}`;
        pairCounts[key] = (pairCounts[key] || 0) + 1;
    }

    const titles = new Map();
    for (const track of trackList) {
        const lang = languageCodeEndonym(track.lang, needsRegion.has(track.lang)) ?? "";
        const title = track.title ?? "";
        const hasLang = lang !== "";
        const hasTitle = title !== "";

        if (!hasLang && !hasTitle) {
            titles.set(track, `Track ${track.id}`);
            continue;
        }
        if (!hasLang) {
            titles.set(track, title);
            continue;
        }

        const isOnlyOneWithLang = langCounts[track.lang] === 1;
        if (isOnlyOneWithLang) {
            titles.set(track, lang);
            continue;
        }

        // Multiple tracks share this language — need disambiguation
        const pairKey = `${lang}\0${title}`;
        const needsNumber = pairCounts[pairKey] > 1 || !hasTitle;
        seen[pairKey] = (seen[pairKey] || 0) + 1;

        if (hasTitle && !needsNumber) {
            const titleIncludesLang = title.toLowerCase().includes(lang.toLowerCase());
            titles.set(track, titleIncludesLang ? title : `${lang} - ${title}`);
        } else if (hasTitle && needsNumber) {
            titles.set(track, `${lang} - ${title} ${seen[pairKey]}`);
        } else {
            titles.set(track, `${lang} - ${seen[pairKey]}`);
        }
    }
    return titles;
}

function languageCodeEndonym(code, includeRegion = false) {
    if (code == undefined || code == "") return undefined;
    try {
        const locale = new Intl.Locale(code);
        const lang = locale.language;
        const display = new Intl.DisplayNames([lang], { type: "language" });
        // Only include region qualifier when there are multiple variants of
        // the same base language (e.g. es-419 and es-ES both present).
        const lookupCode = includeRegion ? locale.toString() : lang;
        const name = display.of(lookupCode) ?? display.of(lang);
        if (!name) return code;
        return name[0].toLocaleUpperCase(lang) + name.slice(1);
    } catch {
        return code;
    }
}
