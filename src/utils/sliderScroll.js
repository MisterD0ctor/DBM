/**
 * Enable mouse-wheel adjustment of any `<input type="range">` in the document.
 * Scrolling up increases the value by `step`, down decreases it. Dispatches
 * an `input` event so existing listeners react as if the user dragged.
 */
export function enableSliderScroll() {
    document.addEventListener(
        "wheel",
        (event) => {
            const slider = event.target.closest?.('input[type="range"]');
            if (!slider || slider.disabled) return;

            event.preventDefault();

            const step = Number(slider.step) || 1;
            const min = Number(slider.min);
            const max = Number(slider.max);
            const direction = event.deltaY < 0 ? 1 : -1;
            const next = Math.min(max, Math.max(min, Number(slider.value) + direction * step));

            if (next === Number(slider.value)) return;

            slider.value = next;
            slider.dispatchEvent(new Event("input", { bubbles: true }));
            slider.dispatchEvent(new Event("change", { bubbles: true }));
        },
        { passive: false },
    );
}
