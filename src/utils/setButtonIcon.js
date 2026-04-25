export function setButtonIcon(buttonId, src) {
    document.getElementById(buttonId)?.querySelector("img")?.setAttribute("src", src);
}

export function setButtonTooltip(buttonId, text) {
    const el = document.getElementById(buttonId)?.querySelector(".tooltip-text");
    if (el) el.textContent = text;
}
