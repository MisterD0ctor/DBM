import * as player from "./player.js";
import * as ui from "./ui/ui.js";
import { closeOnOutsideClick } from "./utils/closeOnOutsideClick.js";

/**
 * Parameter definitions mirror the //!PARAM headers in
 * src-tauri/shaders/ambient-border.glsl.
 */
const AMBIENT_PARAMS = [
    { name: "edge_blur", label: "Edge blur", min: 0, max: 0.1, step: 0.001, value: 0.01 },
    { name: "spread", label: "Spread", min: 0.001, max: 2, step: 0.01, value: 1.0 },
    { name: "falloff", label: "Falloff", min: 0.001, max: 10, step: 0.1, value: 4 },
    { name: "falloff_softness", label: "Falloff softness", min: 0, max: 2, step: 0.01, value: 0.2 },
];

let inFlight = false;
let pending = false;

function pushOptions() {
    if (inFlight) {
        pending = true;
        return;
    }
    inFlight = true;
    const snapshot = AMBIENT_PARAMS.map(({ name, value }) => ({ name, value }));
    player
        .setBorderShaderOptions(snapshot)
        .catch((err) => console.warn("set shader options:", err))
        .finally(() => {
            inFlight = false;
            if (pending) {
                pending = false;
                pushOptions();
            }
        });

    drawPreviews();
}

export async function persistParams() {
    const values = Object.fromEntries(AMBIENT_PARAMS.map(({ name, value }) => [name, value]));
    values.enabled = await ambientEnabled();
    player.saveAmbientParams(values).catch((err) => console.warn("save ambient params:", err));
}

async function ambientEnabled() {
    return "shader" === (await player.getBorderBackground());
}

export async function toggleAmbient(force) {
    let enabled = force ?? !(await ambientEnabled());
    player
        .setBorderBackground(enabled ? "shader" : "color")
        .catch((err) => console.warn("toggle ambient:", err));
    persistParams();
    return enabled;
}

/** Sync UI + saved params with the current value of mpv's `border-background`. */
export function applyState(borderBackground) {
    ui.toggleAmbient(borderBackground === "shader");
    persistParams();
}

function buildSliders() {
    const container = document.querySelector("#ambient-menu .ambient-sliders");
    if (!container) return;
    container.innerHTML = "";
    for (const param of AMBIENT_PARAMS) {
        // Sliders all run on a unified [0, 1] scale; the shader still gets
        // the value mapped back into the param's own min/max range.
        const toSlider = (v) => (v - param.min) / (param.max - param.min);
        const fromSlider = (v) => param.min + v * (param.max - param.min);

        const item = document.createElement("div");
        item.className = "menu-item";
        item.setAttribute("data-slider-wrap", "");
        item.innerHTML = `
            <canvas class="ambient-slider-preview" id="${param.name}" width="24px" height="24px"></canvas>
            <div class="ambient-row">
                <div class="ambient-row-head">
                    <span class="ambient-row-label">${param.label}</span>
                    <span class="ambient-row-value"></span>
                </div>
                <input
                    class="ambient-slider"
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value="${toSlider(param.value)}"
                />
            </div>
        `;
        const slider = item.querySelector("input");
        const valueEl = item.querySelector(".ambient-row-value");
        const setProgressVar = (v) => {
            slider.style.setProperty("--slider-progress", `${v * 100}%`);
        };
        const formatValue = (v) => Number(v).toFixed(2);
        const initial = toSlider(param.value);
        valueEl.textContent = formatValue(initial);
        setProgressVar(initial);

        slider.addEventListener("input", () => {
            const sliderV = Number(slider.value);
            param.value = fromSlider(sliderV);
            valueEl.textContent = formatValue(sliderV);
            setProgressVar(sliderV);
            pushOptions();
            persistParams();
        });

        // Let the whole row act as the slider: clicking/dragging anywhere on
        // .ambient-row updates the slider's value based on horizontal pointer
        // position. The value is computed against the slider's own rect so the
        // visual thumb position matches the click.
        const setFromPointer = (clientX) => {
            const rect = slider.getBoundingClientRect();
            if (rect.width <= 0) return;
            const v = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
            slider.value = v;
            slider.dispatchEvent(new Event("input", { bubbles: true }));
        };
        const onMove = (e) => setFromPointer(e.clientX);
        const onUp = () => {
            window.removeEventListener("pointermove", onMove);
            window.removeEventListener("pointerup", onUp);
        };
        item.addEventListener("pointerdown", (e) => {
            if (e.button !== 0) return;
            e.preventDefault();
            setFromPointer(e.clientX);
            window.addEventListener("pointermove", onMove);
            window.addEventListener("pointerup", onUp);
        });

        container.appendChild(item);
    }
}

export async function initAmbientMenu() {
    try {
        const saved = await player.loadAmbientParams();
        if (saved && typeof saved === "object") {
            for (const param of AMBIENT_PARAMS) {
                const v = Number(saved[param.name]);
                if (Number.isFinite(v)) param.value = v;
            }
            toggleAmbient(saved.enabled);
        }
    } catch (err) {
        console.warn("load ambient params:", err);
    }

    buildSliders();

    const btn = document.getElementById("btn-ambient");
    const menu = document.getElementById("ambient-menu");
    const toggleBtn = document.getElementById("ambient-toggle");

    btn?.addEventListener("click", (e) => {
        ui.toggleAmbientMenu();
    });

    toggleBtn?.addEventListener("click", (e) => {
        toggleAmbient();
    });

    closeOnOutsideClick(menu, btn, () => ui.toggleAmbientMenu(false));

    // Push initial param values so the shader matches the UI state.
    pushOptions();
}

function drawPreviews() {
    drawEdgeBlurPreview();
    drawSpreadPreview();
    drawFalloffPreview();
    drawFalloffSoftnessPreview();
}

function drawEdgeBlurPreview() {
    const param = AMBIENT_PARAMS[0];
    const canvas = document.getElementById(param.name);
    if (!(canvas instanceof HTMLCanvasElement)) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const width = canvas.width;
    const height = canvas.height;
    const size = Math.max(width, height);
    const centerY = height / 2;
    ctx.clearRect(0, 0, width, height);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const dy = y - centerY;
            const weight = Math.max(0, gaussianFalloff((dy / size) * 0.5, param.value + 0.01));
            ctx.fillStyle = `rgba(255, 255, 255, ${Math.max(0, weight)})`;
            ctx.fillRect(x, y, 1, 1);
        }
    }
}

function drawSpreadPreview() {
    const param = AMBIENT_PARAMS[1];
    const canvas = document.getElementById(param.name);
    if (!(canvas instanceof HTMLCanvasElement)) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const width = canvas.width;
    const height = canvas.height;
    const size = Math.max(width, height);
    const centerY = height / 2;
    ctx.clearRect(0, 0, width, height);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const dy = y - centerY;
            const weight =
                spreadFalloffWeight(dy, x, param.value) * distanceFalloff(Math.hypot(x, dy) / size);
            ctx.fillStyle = `rgba(255, 255, 255, ${Math.max(0, weight)})`;
            ctx.fillRect(x, y, 1, 1);
        }
    }
}

function drawFalloffPreview() {
    const param = AMBIENT_PARAMS[2];
    const canvas = document.getElementById(param.name);
    if (!(canvas instanceof HTMLCanvasElement)) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const width = canvas.width;
    const height = canvas.height;
    const size = Math.max(width, height);
    const centerX = width / 2;
    const centerY = height / 2;
    ctx.clearRect(0, 0, width, height);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const weight = distanceFalloff((x / size) * param.value);
            ctx.fillStyle = `rgba(255, 255, 255, ${Math.max(0, weight)})`;
            ctx.fillRect(x, y, 1, 1);
        }
    }
}

function drawFalloffSoftnessPreview() {
    const param = AMBIENT_PARAMS[3];
    const canvas = document.getElementById(param.name);
    if (!(canvas instanceof HTMLCanvasElement)) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const width = canvas.width;
    const height = canvas.height;
    const size = Math.max(width, height);
    ctx.clearRect(0, 0, width, height);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const weight = softDistanceFalloff((x / size) * 2, param.value);
            ctx.fillStyle = `rgba(255, 255, 255, ${weight})`;
            ctx.fillRect(x, y, 1, 1);
        }
    }
}

function gaussianFalloff(x, sigma) {
    return Math.exp(-0.5 * Math.pow(x / sigma, 2));
}

function spreadFalloffWeight(x, d, spread) {
    return (d * spread) / Math.hypot(x, d * spread);
}

function distanceFalloff(x) {
    return 1 / (x * x + 2 * Math.abs(x) + 1);
}

function softDistanceFalloff(x, softness) {
    if (softness == 0.0) {
        return 1 / ((x + 1) * (x + 1));
    } else {
        const c = 1 / softness;
        const th = Math.abs(c * x) < 5 ? Math.tanh(c * x) : Math.sign(x);
        const den = x * th + 1;
        return 1 / (den * den);
    }
}
