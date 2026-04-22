import * as player from "./player.js";

const { convertFileSrc } = window.__TAURI__.core;

/**
 * @type {null | {path: string, sprite: string, grid: number, tileW: number, tileH: number}}
 */
let current = null;
let currentPath = null;

function normalize(info) {
    return {
        path: info.path,
        sprite: info.sprite,
        grid: info.grid,
        tileW: info.tile_w,
        tileH: info.tile_h,
    };
}

function getElements() {
    return {
        box: document.getElementById("seek-preview"),
        img: document.getElementById("seek-preview-image"),
    };
}

function applyAvailability() {
    const { box, img } = getElements();
    if (!box || !img) return;
    if (!current) {
        img.removeAttribute("src");
        box.classList.add("no-preview");
        return;
    }
    const src = convertFileSrc(current.sprite);
    console.log("[preview] sprite", current.sprite, "→", src);
    img.onload = () => console.log("[preview] image loaded", img.naturalWidth, "x", img.naturalHeight);
    img.onerror = (e) => console.warn("[preview] image load failed", src, e);
    img.src = src;
    box.classList.remove("no-preview");
    box.style.setProperty("--tile-w", `${current.tileW}px`);
    box.style.setProperty("--tile-h", `${current.tileH}px`);
    box.style.setProperty("--sprite-w", `${current.tileW * current.grid}px`);
    box.style.setProperty("--sprite-h", `${current.tileH * current.grid}px`);
}

/**
 * Update the tile shown in the preview box for a given 0..1 fraction.
 * @param {number} fraction
 */
export function showAtFraction(fraction) {
    if (!current) return;
    const total = current.grid * current.grid;
    const idx = Math.min(total - 1, Math.max(0, Math.floor(fraction * total)));
    const col = idx % current.grid;
    const row = Math.floor(idx / current.grid);
    const { box } = getElements();
    if (!box) return;
    box.style.setProperty("--tile-x", `${-col * current.tileW}px`);
    box.style.setProperty("--tile-y", `${-row * current.tileH}px`);
}

export function hasPreview() {
    return current !== null;
}

/**
 * Call when the loaded file path changes. Fetches any cached preview and
 * the next preview-ready event for the same path will replace it.
 * @param {string | null} path
 */
export async function setCurrentVideo(path) {
    currentPath = path;
    current = null;
    applyAvailability();

    if (!path) return;
    try {
        const cached = await player.getPreview(path);
        if (cached && cached.path === currentPath) {
            current = normalize(cached);
            applyAvailability();
        }
    } catch (e) {
        console.warn("getPreview failed:", e);
    }
}

/** Wire up the Rust → JS event channel. Call once at startup. */
export function init() {
    player.onPreviewReady((info) => {
        if (info.path === currentPath) {
            current = normalize(info);
            applyAvailability();
        }
    });
}
