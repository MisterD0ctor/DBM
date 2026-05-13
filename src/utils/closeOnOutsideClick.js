/**
 * Run `close` when a click lands outside both `menu` and its trigger `btn`.
 * Standard click-outside-to-dismiss pattern for popover menus.
 *
 * @param {Element} menu  The menu element.
 * @param {Element} btn   The button that opens the menu (so clicks on it are ignored).
 * @param {() => void} close
 */
export function closeOnOutsideClick(menu, btn, close) {
    document.addEventListener("click", (event) => {
        if (!menu.contains(event.target) && !btn.contains(event.target)) {
            close();
        }
    });
}
