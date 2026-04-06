import { lerp } from "./utils/lerp.js";
import * as player from "./player.js";
import * as ui from "./ui/ui.js";

let ambientEnabled = true;
ui.setAmbientOverlay(ambientEnabled);

export function toggleAmbient() {
    ambientEnabled = !ambientEnabled;
    ui.setAmbientOverlay(ambientEnabled);
    player.setProperty("border-background", ambientEnabled ? "blur" : "color");
    updateAspectRatio();
}

export function updateAspectRatio() {
    player
        .getProperty("video-params/aspect", "double")
        .catch(() => 0)
        .then((aspect) => {
            player.getProperty("panscan", "double").then((panscan) => {
                const uiAspect = ui.getAspectRatio();
                ui.setAmbientAspectRatio(lerp(aspect, uiAspect, panscan));
            });
        });
}
