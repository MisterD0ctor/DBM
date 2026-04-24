import * as player from "./player.js";
import * as ui from "./ui/ui.js";

/**
 * Parameter definitions mirror the //!PARAM headers in
 * src-tauri/shaders/ambient-border.glsl.
 */
const AMBIENT_PARAMS = [
    { name: "edge_blur", label: "Edge blur", min: 0, max: 0.1, step: 0.001, value: 0.01 },
    { name: "spread", label: "Spread", min: 0.01, max: 2, step: 0.01, value: 1.0 },
    { name: "falloff", label: "Falloff", min: 0, max: 10, step: 0.1, value: 4 },
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
}

export async function persistParams() {
    const values = Object.fromEntries(AMBIENT_PARAMS.map(({ name, value }) => [name, value]));
    values.enabled = await ambientEnabled();
    player.saveAmbientParams(values).catch((err) => console.warn("save ambient params:", err));
}

async function ambientEnabled() {
    return "shader" === (await player.getBorderBackground());
}

export function toggleAmbient(force) {
    if (force === undefined) {
        ambientEnabled().then((enabled) => {
            player
                .setBorderBackground(enabled ? "color" : "shader")
                .catch((err) => console.warn("toggle ambient:", err));
        });
    } else {
        player
            .setBorderBackground(force ? "shader" : "color")
            .catch((err) => console.warn("toggle ambient:", err));
    }
    persistParams();
}

function buildSliders() {
    const container = document.querySelector("#ambient-menu .ambient-sliders");
    if (!container) return;
    container.innerHTML = "";
    for (const param of AMBIENT_PARAMS) {
        const row = document.createElement("div");
        row.className = "ambient-row";
        row.innerHTML = `
            <div class="ambient-row-head">
                <span class="ambient-row-label">${param.label}</span>
                <span class="ambient-row-value"></span>
            </div>
            <input
                class="ambient-slider"
                type="range"
                min="${param.min}"
                max="${param.max}"
                step="${param.step}"
                value="${param.value}"
            />
        `;
        const slider = row.querySelector("input");
        const valueEl = row.querySelector(".ambient-row-value");
        const setProgressVar = (v) => {
            const pct = ((v - param.min) / (param.max - param.min)) * 100;
            slider.style.setProperty("--slider-progress", `${pct}%`);
        };
        const formatValue = (v) => (param.step >= 1 ? v : Number(v).toFixed(3));
        valueEl.textContent = formatValue(param.value);
        setProgressVar(param.value);

        slider.addEventListener("input", () => {
            const v = Number(slider.value);
            param.value = v;
            valueEl.textContent = formatValue(v);
            setProgressVar(v);
            pushOptions();
            persistParams();
        });
        container.appendChild(row);
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

    document.addEventListener("click", (e) => {
        if (!menu.contains(e.target) && e.target !== btn && !btn.contains(e.target)) {
            ui.toggleAmbientMenu(false);
        }
    });

    // Push initial param values so the shader matches the UI state.
    pushOptions();
}
