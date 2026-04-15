import { setButtonIcon } from "../utils/setButtonIcon.js";

let state = { isMuted: false, volume: 100 };

export function setMute(isMuted) {
    state.isMuted = isMuted;
    setButtonIcon(
        "btn-mute",
        isMuted ? "assets/icons/normal-straight/volume-mute.svg" : getVolumeIcon(state.volume),
    );
}

export function setVolume(volume) {
    state.volume = volume;

    document.getElementById("volume-slider").value = volume;
    document.getElementById("volume-display").innerText = volume;

    if (!state.isMuted) {
        setButtonIcon("btn-mute", getVolumeIcon(volume));
    }
}

function getVolumeIcon(volume) {
    if (volume > 133) {
        return "assets/icons/normal-straight/volume-up.svg";
    } else if (volume > 66) {
        return "assets/icons/normal-straight/volume.svg";
    } else {
        return "assets/icons/normal-straight/volume-down.svg";
    }
}
