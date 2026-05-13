export function getVolumeIcon(volume) {
    volume = parseFloat(volume);
    if (volume > 133) {
        return "assets/icons/volume-up.svg";
    } else if (volume > 66) {
        return "assets/icons/volume.svg";
    } else if (volume > 0) {
        return "assets/icons/volume-down.svg";
    } else {
        return "assets/icons/volume-none.svg";
    }
}
