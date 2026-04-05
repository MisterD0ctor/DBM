const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

/**
 * @typedef {'play'|'pause'|'next'|'previous'|'stop'} SmtcCommand
 * @typedef {(cmd: SmtcCommand) => void} SmtcCommandHandler
 */

let _unlisten = null;
let _initialized = false;

/**
 * Initialize SMTC and start listening for media key / transport control events.
 * Call this once when your player mounts.
 *
 * @param {SmtcCommandHandler} onCommand - called whenever the user presses a media button
 * @returns {Promise<void>}
 */
export async function setup(onCommand) {
    if (_initialized) return;

    await invoke("setup_smtc");

    _unlisten = await listen("smtc-command", (event) => {
        onCommand(event.payload);
    });

    _initialized = true;
}

/**
 * Update the playback status shown in the transport controls.
 *
 * @param {boolean} playing
 * @returns {Promise<void>}
 */
export async function setPlayback(playing) {
    if (!_initialized) throw new Error("SMTC not initialized — call setup() first");
    await invoke("update_smtc_playback", { playing });
}

/**
 * Update the media metadata shown in the transport controls overlay.
 *
 * @param {string} title    - e.g. video file name or show title
 * @param {string} subtitle - e.g. episode name or author
 * @returns {Promise<void>}
 */
export async function setMetadata(title, subtitle = "") {
    if (!_initialized) throw new Error("SMTC not initialized — call setup() first");
    await invoke("update_smtc_metadata", { title: title ?? "", subtitle: subtitle ?? "" });
}

/**
 * Update both playback status and metadata atomically.
 * Prefer this over calling setSmtcPlayback + setSmtcMetadata separately
 * to avoid a brief inconsistent state.
 *
 * @param {{ playing: boolean, title: string, subtitle?: string }} opts
 * @returns {Promise<void>}
 */
export async function update({ playing, title, subtitle = "" }) {
    if (!_initialized) throw new Error("SMTC not initialized — call setup() first");
    await invoke("update_smtc", { playing, title, subtitle });
}

/**
 * Tear down SMTC and remove the event listener.
 * Call this when your player unmounts.
 *
 * @returns {Promise<void>}
 */
export async function teardown() {
    if (!_initialized) return;
    await invoke("teardown_smtc");
    _unlisten?.();
    _unlisten = null;
    _initialized = false;
}
