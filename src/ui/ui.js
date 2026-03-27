import { formatTime } from "../utils/formatTime.js";

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
  document.getElementById("ambient-overlay")?.classList.toggle("hidden", !isEnabled);
  setAmbient(isEnabled);
}

/**
 * Push new edge colors into the four ambient bars.
 * Each color is an [r, g, b] tuple (0–255).
 *
 * The bars use a CSS radial-gradient that bleeds from the edge color
 * toward transparent, creating a soft glow rather than a hard band.
 */
export function setAmbientColors({ top, bottom, left, right }) {
  if (top) {
    document
      .getElementById("ambient-top")
      .style.setProperty("--ambient-color", `rgb(${top[0]},${top[1]},${top[2]})`);
  }
  if (bottom) {
    document
      .getElementById("ambient-bottom")
      .style.setProperty("--ambient-color", `rgb(${bottom[0]},${bottom[1]},${bottom[2]})`);
  }
  if (left) {
    document
      .getElementById("ambient-left")
      .style.setProperty("--ambient-color", `rgb(${left[0]},${left[1]},${left[2]})`);
  }
  if (right) {
    document
      .getElementById("ambient-right")
      .style.setProperty("--ambient-color", `rgb(${right[0]},${right[1]},${right[2]})`);
  }
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

export function toggleTracksMenu() {
  document.getElementById("tracks-menu")?.classList.toggle("hidden");
}

export function hideTracksMenu() {
  document.getElementById("tracks-menu")?.classList.add("hidden");
}

export function showTracksMenu() {
  document.getElementById("tracks-menu")?.classList.remove("hidden");
}

export function populateSubtitlesMenu(subtitles, onSelect, onDisable) {
  const menu = document.getElementById("subtitles-menu");
  menu.innerHTML = "";

  const title = document.createElement("div");
  title.className = "title";
  title.textContent = "Subtitles";
  menu.appendChild(title);

  const noneItem = document.createElement("div");
  noneItem.className = "item";
  noneItem.textContent = "Off";
  noneItem.onclick = () => {
    onDisable();
    hideTracksMenu();
  };
  menu.appendChild(noneItem);

  for (const sub of subtitles) {
    const item = document.createElement("div");
    item.className = "item";
    item.textContent = languageCodeEndonym(sub.lang) ?? sub.title ?? `Track ${sub.id}`;
    item.onclick = () => {
      onSelect(sub.id);
      hideTracksMenu();
    };
    menu.appendChild(item);
  }
}

export function populateAudioTracksMenu(audioTracks, onSelect) {
  const menu = document.getElementById("audio-tracks-menu");
  menu.innerHTML = "";

  const title = document.createElement("div");
  title.className = "title";
  title.textContent = "Audio";
  menu.appendChild(title);

  for (const track of audioTracks) {
    const item = document.createElement("div");
    item.className = "item";
    item.textContent = languageCodeEndonym(track.lang) ?? track.title ?? `Track ${track.id}`;
    item.onclick = () => {
      onSelect(track.id);
      hideTracksMenu();
    };
    menu.appendChild(item);
  }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

function languageCodeEndonym(code) {
  if (!code) return null;
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
