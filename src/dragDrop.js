import * as player from "./player.js";

const { getCurrentWebview } = window.__TAURI__.webview;

getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const videoPath = event.payload.paths[0]?.toString();
    if (videoPath) player.loadVideo(videoPath);
});
