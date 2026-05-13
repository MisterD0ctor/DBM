import * as ui from "./ui/ui.js";

const { getCurrentWindow } = window.__TAURI__.window;

// Saved across enter→exit so we can restore the window when leaving fullscreen
// (Tauri reports inner size / outer position, so we set those on the way back).
let savedWindowState = {
    isMaximized: false,
    position: undefined,
    size: undefined,
};

export async function setFullscreen(enable) {
    const win = getCurrentWindow();

    if (enable) {
        savedWindowState = {
            isMaximized: await win.isMaximized(),
            position: await win.outerPosition(),
            size: await win.innerSize(),
        };
        await win.maximize();
        await win.setFullscreen(true);
    } else {
        await win.setFullscreen(false);
        // Guard against Escape being pressed before fullscreen was ever entered:
        // savedWindowState.size/position would be undefined and Tauri's Size
        // wrapper crashes accessing this.size.type.
        if (!savedWindowState.isMaximized && savedWindowState.size && savedWindowState.position) {
            await win.setSize(savedWindowState.size);
            await win.setPosition(savedWindowState.position);
            await win.unmaximize();
        }
    }

    ui.toggleFullscreen(enable);
}

export async function toggleFullscreen() {
    const isFullscreen = await getCurrentWindow().isFullscreen();
    setFullscreen(!isFullscreen);
    return !isFullscreen;
}
