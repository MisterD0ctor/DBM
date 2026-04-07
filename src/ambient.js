import { lerp } from "./utils/lerp.js";
import * as player from "./player.js";
import * as ui from "./ui/ui.js";

export function toggleAmbient() {
    player
        .getProperty("border-background")
        .then((bb) => player.setProperty("border-background", bb === "blur" ? "color" : "blur"));
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
