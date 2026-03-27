const { getCurrentWebview } = window.__TAURI__.webview;
const { getCurrentWindow } = window.__TAURI__.window;
const { resolveResource } = window.__TAURI__.path;
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
import { isVideoFile } from "./utils/isVideoFile.js";
import { lerp } from "./utils/lerp.js";
import * as mpv from "./libmpv/libmpv.js";
import * as ui from "./ui/ui.js";

// --- Constants ----------------------------------------------------------------

const SEEK_SECONDS = 10;
const OVERLAY_HIDE_DELAY_MS = 3000;
const DOUBLE_CLICK_DELAY_MS = 250;

const OBSERVED_PROPERTIES = [
  ["time-pos", "double", "none"],
  ["percent-pos", "double", "none"],
  ["duration", "double", "none"],
  ["filename", "string", "none"],
  ["pause", "flag"],
  ["mute", "flag"],
  ["volume", "double"],
  ["speed", "double"],
  ["eof-reached", "flag", "none"],
  ["panscan", "double", "none"],
];

// --- Variables ----------------------------------------------------------------

let duration = 0;

// --- mpv init -----------------------------------------------------------------

const mpvConfig = {
  // prettier-ignore
  initialOptions: {
    "border-background": "blur",
    "background-blur-radius": "50",
    "deband": "yes",
    "deband-iterations": "8",
    "force-window": "yes",
    "hwdec": "auto-safe",
    "keep-open": "yes",
    "pause": "no",
    "sub-visibility": "no",
    "vo": "gpu-next",
    // "sub-ass-vsfilter-aspect-compat": "no",
    // "sub-use-margins": "yes",
    // "sub-ass-use-video-data": "no",
    // "stretch-image-subs-to-screen": "yes", // handles PGS/VOBSUB
  },
  observedProperties: OBSERVED_PROPERTIES,
};

try {
  console.log("MPV loading");
  await mpv.init(mpvConfig);
  Object.keys(mpvConfig.initialOptions).forEach(async (key) => {
    await mpv.setProperty(key, mpvConfig.initialOptions[key]);
  });
  await mpv.setProperty("pause", "no");
  duration = await mpv.getProperty("duration", "double").catch(() => 0);
  ui.setDuration(duration);
  ui.setFilename(await mpv.getProperty("filename", "string").catch(() => ""));
  setAmbientAspectRatio();
  await populateTracksMenu();
  console.log("MPV initialized");
} catch (err) {
  console.error("MPV initialization failed:", err);
}

// --- Property observation -----------------------------------------------------

mpv.observeProperties(OBSERVED_PROPERTIES, ({ name, data }) => {
  // prettier-ignore
  switch (name) {
    case "time-pos":     ui.setCurrentTime(data);         break;
    case "percent-pos":  ui.setProgress(data);            break;
    case "duration":     ui.setDuration(duration = data); break;
    case "filename":     ui.setFilename(data);            break;
    case "pause":        ui.setPause(data);               break;
    case "mute":         ui.setMute(data);                break;
    case "volume":       ui.setVolume(data);              break;
    case "speed":        /* no UI yet */                  break;
    case "panscan":      ui.setPanscan(data); setAmbientAspectRatio(); break;
    case "eof-reached":  console.log("EOF");              break;
    default: console.warn("Unhandled property:", name);
  }
});

// --- Window event listeners -------------------------------------------------

listen("tauri://resize", () => {
  setAmbientAspectRatio();
});

// --- Playlist helpers ---------------------------------------------------------

function loadPlaylist(playlistPath, fileIndex) {
  mpv.command("loadlist", [playlistPath, "replace"]).then(() => {
    mpv.command("playlist-play-index", [fileIndex]);
    // Small delay to allow the file to load before unpausing / refreshing tracks
    setTimeout(() => {
      mpv.setProperty("pause", "no");
      populateTracksMenu();
    }, 100);
  });
}

async function populateTracksMenu() {
  const tracks = JSON.parse(await mpv.getProperty("track-list", "string"));
  const subtitles = tracks.filter((t) => t.type === "sub");
  const audioTracks = tracks.filter((t) => t.type === "audio");

  ui.populateSubtitlesMenu(
    subtitles,
    (id) => {
      mpv.setProperty("sid", id.toString()).then(() => mpv.setProperty("sub-visibility", "yes"));
    },
    () => mpv.setProperty("sub-visibility", "no"),
  );

  ui.populateAudioTracksMenu(audioTracks, (id) => {
    mpv.setProperty("aid", id.toString());
  });
}

// --- Drag & drop --------------------------------------------------------------

getCurrentWebview().onDragDropEvent((event) => {
  if (event.payload.type !== "drop") return;
  const videoPath = event.payload.paths[0]?.toString();
  if (videoPath && isVideoFile(videoPath)) {
    invoke("playlist_from_video", { videoPath }).then(([playlistPath, fileIndex]) => {
      loadPlaylist(playlistPath, fileIndex);
    });
  }
});

// --- Overlay show/hide -------------------------------------------------------

let hideTimer = null;

function showOverlay() {
  ui.setOverlay(true);
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => ui.setOverlay(false), OVERLAY_HIDE_DELAY_MS);
}

const overlay = document.getElementById("overlay");
overlay.addEventListener("mousemove", showOverlay);
overlay.addEventListener("mouseenter", showOverlay);
overlay.addEventListener("mousedown", showOverlay);
overlay.addEventListener("mouseleave", () => ui.setOverlay(false));

// --- Playback controls --------------------------------------------------------

function togglePause() {
  mpv.getProperty("pause", "bool").then((paused) => {
    mpv.setProperty("pause", paused === "yes" ? "no" : "yes");
  });
}

function toggleMute() {
  mpv.getProperty("mute", "bool").then((muted) => {
    mpv.setProperty("mute", muted === "yes" ? "no" : "yes");
  });
}

function toggleFullscreen() {
  getCurrentWindow()
    .isFullscreen()
    .then((isFs) => {
      getCurrentWindow().setFullscreen(!isFs);
      ui.setFullscreen(!isFs);
    });
}

function togglePanscan() {
  mpv.getProperty("panscan", "double").then((panscan) => {
    const next = panscan === 1 ? 0 : 1;
    mpv.setProperty("panscan", next);
  });
}

function seek(seconds) {
  mpv.getProperty("time-pos", "double").then((pos) => {
    mpv.setProperty("time-pos", pos + seconds);
  });
}

// --- Ambient lighting ---------------------------------------------------------

let ambientEnabled = true;
ui.setAmbientOverlay(ambientEnabled);

function toggleAmbient() {
  ambientEnabled = !ambientEnabled;

  if (ambientEnabled) {
    ui.setAmbientOverlay(true);
    mpv.setProperty("border-background", "blur");
  } else {
    ui.setAmbientOverlay(false);
    mpv.setProperty("border-background", "color");
  }

  setAmbientAspectRatio();
}

function setAmbientAspectRatio() {
  mpv
    .getProperty("video-params/aspect", "double")
    .catch(() => 0)
    .then((aspect) => {
      mpv.getProperty("panscan", "double").then((panscan) => {
        const uiAspect = ui.getAspectRatio();
        const effectiveAspect = lerp(aspect, uiAspect, panscan);

        ui.setAmbientAspectRatio(effectiveAspect);
      });
    });
}

// --- Button wiring ------------------------------------------------------------

document.getElementById("open-path-button").onclick = () =>
  invoke("playlist_from_path_dialog").then(([playlistPath, fileIndex]) =>
    loadPlaylist(playlistPath, fileIndex),
  );

document.getElementById("previous-button").onclick = () => {
  mpv.command("playlist-prev").then(() =>
    setTimeout(() => {
      mpv.setProperty("pause", "no");
      populateTracksMenu();
    }, 100),
  );
};

document.getElementById("next-button").onclick = () => {
  mpv.command("playlist-next").then(() =>
    setTimeout(() => {
      mpv.setProperty("pause", "no");
      populateTracksMenu();
    }, 100),
  );
};

document.getElementById("seek-backward-button").onclick = () => seek(-SEEK_SECONDS);
document.getElementById("seek-forward-button").onclick = () => seek(SEEK_SECONDS);
document.getElementById("play-button").onclick = togglePause;
document.getElementById("ambient-button").onclick = toggleAmbient;
document.getElementById("panscan-button").onclick = togglePanscan;
document.getElementById("mute-button").onclick = toggleMute;
document.getElementById("fullscreen-button").onclick = toggleFullscreen;

const volumeSlider = document.getElementById("volume-slider");
volumeSlider.addEventListener("input", () => mpv.setProperty("volume", volumeSlider.value));

// --- Tracks menu --------------------------------------------------------------

const tracksMenu = document.getElementById("tracks-menu");
const tracksButton = document.getElementById("tracks-button");

tracksButton.onclick = () => ui.toggleTracksMenu();

document.addEventListener("click", (event) => {
  if (!tracksMenu.contains(event.target) && !tracksButton.contains(event.target)) {
    ui.hideTracksMenu();
  }
});

// --- Seek bar -----------------------------------------------------------------

const seekEl = document.querySelector(".seek");

seekEl.addEventListener("click", (event) => {
  const percent = (event.offsetX / seekEl.getBoundingClientRect().width) * 100;
  mpv.setProperty("percent-pos", percent);
});

seekEl.addEventListener("mousemove", (event) => {
  const fraction = event.offsetX / seekEl.getBoundingClientRect().width;
  ui.setSeekTimeHighlight(true, fraction * duration, fraction * 100);
});

seekEl.addEventListener("mouseleave", () => ui.setSeekTimeHighlight(false));

overlay.addEventListener("mousemove", () => {
  if (!seekEl.matches(":hover")) ui.setSeekTimeHighlight(false);
});

// --- Click vs double-click ----------------------------------------------------

let clickTimeout;

document.querySelector(".interactive").addEventListener("click", (e) => {
  if (e.detail === 1) {
    clickTimeout = setTimeout(togglePause, DOUBLE_CLICK_DELAY_MS);
  } else if (e.detail === 2) {
    clearTimeout(clickTimeout);
    toggleFullscreen();
  }
});

// --- Keyboard shortcuts -------------------------------------------------------

document.addEventListener("keydown", (e) => {
  // prettier-ignore
  switch (e.code) {
    case "Space":       togglePause();       break;
    case "KeyF":        toggleFullscreen();  break;
    case "KeyM":        toggleMute();        break;
    case "KeyT":        togglePanscan();     break;
    case "ArrowRight":  seek(SEEK_SECONDS);  break;
    case "ArrowLeft":   seek(-SEEK_SECONDS); break;
  }
});
