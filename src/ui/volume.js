import { setButtonIcon, setButtonTooltip } from "../utils/setButtonIcon.js";

let state = { isMuted: false, volume: 100 };

export function setMute(isMuted) {
    state.isMuted = isMuted;
    setButtonIcon(
        "btn-mute",
        isMuted ? "assets/icons/volume-mute.svg" : getVolumeIcon(state.volume),
    );
    setButtonTooltip("btn-mute", isMuted ? "Unmute" : "Mute");
    document.getElementById("volume-group").classList.toggle("muted", isMuted);
}

export function setVolume(volume) {
    state.volume = volume;

    const slider = document.getElementById("volume-slider");
    slider.value = volume;
    const max = Number(slider.max) || 200;
    slider.style.setProperty("--volume-progress", `${(volume / max) * 100}%`);
    document.getElementById("volume-display").innerText = volume;

    if (!state.isMuted) {
        setButtonIcon("btn-mute", getVolumeIcon(volume));
    }
}

function getVolumeIcon(volume) {
    if (volume > 133) {
        return "assets/icons/volume-up.svg";
    } else if (volume > 66) {
        return "assets/icons/volume.svg";
    } else {
        return "assets/icons/volume-down.svg";
    }
}
