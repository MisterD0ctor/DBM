import { lerp } from "./utils/lerp.js";
import * as player from "./player.js";
import * as ui from "./ui/ui.js";

let resizeTimer;
let isBlur;

export function toggleAmbient() {
    player
        .getProperty("border-background")
        .then((bb) => player.setProperty("border-background", bb === "blur" ? "color" : "blur"));
}

export function updateAspectRatio() {
    clearTimeout(resizeTimer);

    resizeTimer = setTimeout(() => {
        internalUpdateAspectRatio();
        resizeTimer = undefined;
    }, 300);
}

function internalUpdateAspectRatio() {
    player
        .getProperty("video-params/aspect", "double")
        .catch(() => 0)
        .then((aspect) => {
            player.getProperty("panscan", "double").then((panscan) => {
                const uiAspect = ui.getAspectRatio();
                const effectiveAspect = lerp(aspect, uiAspect, panscan);
                ui.setAmbientAspectRatio(effectiveAspect);
                updateBlurRadius(effectiveAspect, uiAspect);
            });
        });
}

async function updateBlurRadius(videoAspect, containerAspect) {
    if (videoAspect <= 0) return;

    const [vidW, vidH] = await Promise.all([
        player.getProperty("video-params/w", "double"),
        player.getProperty("video-params/h", "double"),
    ]);
    if (!vidW || !vidH) return;

    let barPx;
    if (videoAspect > containerAspect) {
        barPx = ((1 - containerAspect / videoAspect) / 2) * vidH;
    } else {
        barPx = ((1 - videoAspect / containerAspect) / 2) * vidW;
    }

    const radius = Math.round(barPx * 0.5);
    player.setProperty("background-blur-radius", radius.toString());
}
