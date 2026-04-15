export function setButtonIcon(buttonId, src) {
    document.getElementById(buttonId)?.querySelector("img")?.setAttribute("src", src);
}
