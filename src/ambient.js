import * as player from "./player.js";

export function toggleAmbient() {
    player
        .getProperty("border-background")
        .then((bb) =>
            player.setProperty("border-background", bb === "shader" ? "color" : "shader"),
        );
}
