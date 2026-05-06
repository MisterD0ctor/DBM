/**
 * Reflow toolbar buttons that don't fit alongside the centered playback
 * controls into a secondary "overflow" row above the main toolbar.
 *
 * Goal: keep `.toolbar-center` perfectly centered on the window. The grid
 * `1fr auto 1fr` only stays centered while both side columns can shrink to
 * the available space without pushing the center over. Once the rightmost or
 * leftmost button doesn't fit, we move it up to the overflow row instead of
 * letting it shove the play button off-axis.
 */

const main = {
    start: document.querySelector(".toolbar > .toolbar-start"),
    center: document.querySelector(".toolbar > .toolbar-center"),
    end: document.querySelector(".toolbar > .toolbar-end"),
};
const overflow = {
    row: document.querySelector(".toolbar-overflow"),
    start: document.querySelector(".toolbar-overflow > .toolbar-start"),
    end: document.querySelector(".toolbar-overflow > .toolbar-end"),
};

// Capture the full original sequence of children per side so we can deterministically
// rebuild it on every layout pass regardless of how many were previously moved.
const originalStart = Array.from(main.start.children);
const originalEnd = Array.from(main.end.children);

// Overflow priority: which item drops to the overflow row FIRST. Items earlier
// in this list overflow before later ones (so the most essential stays put).
//
// Start side: the playlist button is the media title — it should never leave
// the main bar while there's a file loaded, so it overflows last. Open-menu
// goes first.
// End side: drop fullscreen first, then panscan, then tracks, then ambient,
// keeping the volume group anchored to the bar last.
const startPriority = ["btn-playlist", "btn-open-menu"];
const endPriority = ["volume-group", "btn-ambient", "btn-tracks", "btn-panscan", "btn-fullscreen"];

function sortByPriority(elements, priorityIds) {
    // Items at index 0 of priorityIds overflow first. Map elements -> their
    // overflow priority (lower = overflows sooner). Unknown ids overflow first.
    return [...elements].sort((a, b) => {
        const aIdx = priorityIds.indexOf(a.id);
        const bIdx = priorityIds.indexOf(b.id);
        return (aIdx === -1 ? -Infinity : aIdx) - (bIdx === -1 ? -Infinity : bIdx);
    });
}

function reset() {
    // Move every tracked child back to its main-bar slot in the original order.
    // This gives us a clean baseline so we can re-measure unbiased by previous runs.
    for (const el of originalStart) main.start.appendChild(el);
    for (const el of originalEnd) main.end.appendChild(el);
    overflow.start.replaceChildren();
    overflow.end.replaceChildren();
}

function reflow() {
    reset();

    // After reset, measure how much the bottom toolbar wants vs. how much it has.
    // We're avoiding center push: each side column gets roughly (toolbarWidth - centerWidth) / 2.
    const toolbar = main.center.parentElement;
    const toolbarStyles = getComputedStyle(toolbar);
    const gap = parseFloat(toolbarStyles.columnGap || toolbarStyles.gap) || 0;

    // Available width for each side column, accounting for the center button group + 2 gaps.
    const available = (toolbar.clientWidth - main.center.offsetWidth - gap * 2) / 2;
    if (!Number.isFinite(available) || available <= 0) return;

    // Helper: while the side overflows its budget, pluck the highest-priority overflow
    // candidate and move it to the overflow row (preserving original order in the row).
    function overflowSide(side, originalOrder, priorityIds, overflowContainer) {
        if (side.scrollWidth <= available) return;

        const candidates = sortByPriority(
            originalOrder.filter((el) => el.parentElement === side),
            priorityIds,
        );
        for (const candidate of candidates) {
            // Re-measure each step — moving one button might be enough.
            if (side.scrollWidth <= available) break;
            overflowContainer.appendChild(candidate);
        }
        // Re-sort overflow row to match the original DOM order so items don't shuffle around.
        const ordered = originalOrder.filter((el) => el.parentElement === overflowContainer);
        for (const el of ordered) overflowContainer.appendChild(el);
    }

    overflowSide(main.start, originalStart, startPriority, overflow.start);
    overflowSide(main.end, originalEnd, endPriority, overflow.end);

    // Hide the overflow row entirely when empty so it doesn't show empty pill containers.
    const hasOverflow = overflow.start.children.length > 0 || overflow.end.children.length > 0;
    overflow.row.classList.toggle("hidden", !hasOverflow);
    overflow.start.classList.toggle("hidden", overflow.start.children.length === 0);
    overflow.end.classList.toggle("hidden", overflow.end.children.length === 0);
}

export function initToolbarOverflow() {
    if (!main.start || !main.center || !main.end || !overflow.row) return;

    // Window-resize is the main signal. .toolbar's columns are 1fr auto 1fr
    // so individual side widths only change when window width does.
    // refreshToolbarOverflow() exists for the rare cases where a side's
    // intrinsic content width changes (e.g. media title load).
    const observer = new ResizeObserver(() => reflow());
    observer.observe(main.center.parentElement);
    reflow();
}

/** Re-run overflow calculation. Call when content inside the toolbar changes
 *  (e.g. the playlist button shows/hides on file load). */
export function refreshToolbarOverflow() {
    reflow();
}
