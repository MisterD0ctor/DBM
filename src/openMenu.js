import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import { closeOnOutsideClick } from "./utils/closeOnOutsideClick.js";

const openMenu = document.getElementById("open-menu");
const openMenuBtn = document.getElementById("btn-open-menu");

openMenuBtn.onclick = () => ui.toggleOpenMenu();

openMenu.addEventListener("click", (e) => {
    const item = e.target.closest(".menu-item");
    if (!item) return;
    ui.toggleOpenMenu(false);
    if (item.dataset.action === "open-file") player.openVideoDialog();
    else if (item.dataset.action === "open-folder") player.openFolderDialog();
});

closeOnOutsideClick(openMenu, openMenuBtn, () => ui.toggleOpenMenu(false));
