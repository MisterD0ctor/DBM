import { parseTvShow } from "../utils/parse.js";
import { cleanSeparators, stripMetadata, stripPath, stripExtension } from "../utils/parse.js";

// --- Menu visibility ---------------------------------------------------------

export function togglePlaylistMenu(force) {
    const menu = document.getElementById("playlist-menu");
    if (force !== undefined) {
        menu.classList.toggle("hidden", !force);
    } else {
        menu.classList.toggle("hidden");
    }
}

// --- Populate ----------------------------------------------------------------

export function populatePlaylistMenu(playlist, activeIndex, isPaused, savedPositions, onSelect) {
    const container = document.getElementById("playlist-items");
    container.innerHTML = "";

    for (let i = 0; i < playlist.length; i++) {
        const path = playlist[i].filename ?? "";
        const progress = savedPositions[path];
        const item = createPlaylistItem(playlist[i], i, activeIndex, isPaused, progress, onSelect);
        container.appendChild(item);
    }
}

function createPlaylistItem(entry, index, activeIndex, isPaused, progress, onSelect) {
    const item = document.createElement("div");
    item.className = "menu-item";
    if (index === activeIndex) {
        item.classList.add("active");
        if (!isPaused) item.classList.add("playing");
    }

    // Number / playback icon container (icon replaces number on active item)
    const indicator = document.createElement("div");
    indicator.className = "playlist-indicator";

    const numberEl = document.createElement("span");
    numberEl.className = "playlist-number";
    numberEl.textContent = index + 1;
    indicator.appendChild(numberEl);

    const statusIcon = document.createElement("img");
    statusIcon.className = "playlist-playback-icon";
    statusIcon.src = "assets/icons/play.svg";
    statusIcon.alt = "";
    indicator.appendChild(statusIcon);

    item.appendChild(indicator);

    // Title content
    const content = document.createElement("div");
    content.className = "playlist-content";

    const textRow = document.createElement("div");
    textRow.className = "playlist-content-text";

    const filename = entry.filename ?? entry.title ?? "";
    const name = stripExtension(stripPath(filename));
    const tvData = parseTvShow(name);

    if (tvData) {
        const showEl = document.createElement("span");
        showEl.className = "playlist-show";
        showEl.textContent = tvData.show;
        textRow.appendChild(showEl);

        const episodeEl = document.createElement("span");
        episodeEl.className = "playlist-episode";
        episodeEl.textContent = `S${tvData.season}:E${tvData.episode}`;
        textRow.appendChild(episodeEl);

        if (tvData.title) {
            const epTitleEl = document.createElement("span");
            epTitleEl.className = "playlist-episode-title";
            epTitleEl.textContent = tvData.title;
            textRow.appendChild(epTitleEl);
        }
    } else {
        const titleEl = document.createElement("span");
        titleEl.className = "playlist-title";
        titleEl.textContent = stripMetadata(cleanSeparators(name));
        textRow.appendChild(titleEl);
    }

    content.appendChild(textRow);

    // Progress bar (from watch-later saved position + cached duration)
    if (progress && progress.start > 0 && progress.duration > 0) {
        const progressBar = document.createElement("div");
        progressBar.className = "playlist-progress";

        const progressFill = document.createElement("div");
        progressFill.className = "playlist-progress-fill";
        const pct = Math.min((progress.start / progress.duration) * 100, 100);
        progressFill.style.width = `${pct}%`;

        progressBar.appendChild(progressFill);
        content.appendChild(progressBar);
    }

    item.appendChild(content);

    item.onclick = () => {
        onSelect(index);
        togglePlaylistMenu(false);
    };

    return item;
}

// --- Update active item ------------------------------------------------------

export function setActivePlaylistItem(index, isPaused) {
    const container = document.getElementById("playlist-items");
    if (!container) return;

    for (const item of container.children) {
        const i = [...container.children].indexOf(item);
        const isActive = i === index;
        item.classList.toggle("active", isActive);
        item.classList.toggle("playing", isActive && !isPaused);

        const icon = item.querySelector(".playlist-playback-icon");
        if (icon) {
            icon.src = isPaused && isActive ? "assets/icons/pause.svg" : "assets/icons/play.svg";
        }
    }

    // Scroll active item into view
    const activeItem = container.children[index];
    if (activeItem) {
        activeItem.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
}
