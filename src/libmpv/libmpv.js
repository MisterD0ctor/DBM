const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;
const { listen } = window.__TAURI__.event;

/**
 * Initialize mpv player.
 *
 * @param {MpvConfig} [mpvConfig] - Initialization options.
 * @param {string} [windowLabel] - The label of the target window. Defaults to the current window's label.
 * @returns {Promise<string>} A promise that resolves with the actual window label used for initialization.
 * @throws {Error} Throws an error if mpv initialization fails.
 *
 * @example
 * ```typescript
 * import { init, MpvConfig, MpvObservableProperty } from 'tauri-plugin-libmpv-api';
 *
 * // Note the optional 'none' marker for properties that can be null (e.g., when no file is loaded)
 * const OBSERVED_PROPERTIES = [
 *   ['pause', 'flag'],
 *   ['time-pos', 'double', 'none'],
 *   ['duration', 'double', 'none'],
 *   ['filename', 'string', 'none'],
 * ] as const satisfies MpvObservableProperty[];
 *
 * // mpv configuration
 * const mpvConfig: MpvConfig = {
 *   initialOptions: {
 *     'vo': 'gpu-next',
 *     'hwdec': 'auto-safe',
 *     'keep-open': 'yes',
 *     'force-window': 'yes',
 *   },
 *   observedProperties: OBSERVED_PROPERTIES,
 * };
 *
 * // Initialize mpv
 * await init(mpvConfig);
 * ```
 */
export async function init(mpvConfig, windowLabel) {
    const config = mpvConfig ?? {};
    const winLabel = windowLabel ?? getCurrentWindow().label;
    const transformedConfig = {
        ...config,
        observedProperties: config.observedProperties
            ? Object.fromEntries(config.observedProperties)
            : {},
    };
    return await invoke("plugin:libmpv|init", {
        mpvConfig: transformedConfig,
        windowLabel: winLabel,
    });
}

/**
 * Destroy mpv player.
 *
 * @param {string} [windowLabel] - Target window label, defaults to current window
 * @returns {Promise<void>} A promise that resolves when the operation completes.
 *
 * @example
 * ```typescript
 * import { destroy } from 'tauri-plugin-libmpv-api';
 *
 * await destroy();
 * ```
 */
export async function destroy(windowLabel) {
    if (!windowLabel) {
        windowLabel = getCurrentWindow().label;
    }
    return await invoke("plugin:libmpv|destroy", {
        windowLabel,
    });
}

/**
 * Listen to mpv property change events.
 *
 * @param {ReadonlyArray<MpvObservableProperty>} properties - An array of tuples, where each tuple defines a property to observe.
 * Each tuple is `[propertyName, format]`. An optional third element, `'none'`, can be included
 * (e.g., `['duration', 'double', 'none']`) to signal to TypeScript that the property's value may be null.
 * @param {(event: MpvEventFromProperties<T[number]>) => void} callback - Function to call when a matching property-change event is received.
 * @param {string} [windowLabel] - Target window label, defaults to current window.
 * @returns {Promise<UnlistenFn>} A function to call to stop listening.
 *
 * @example
 * ```typescript
 * import { observeProperties, MpvObservableProperty } from 'tauri-plugin-libmpv-api';
 *
 * const OBSERVED_PROPERTIES = [
 *   ['pause', 'flag'],
 *   ['time-pos', 'double', 'none'],
 *   ['duration', 'double', 'none'],
 *   ['filename', 'string', 'none'],
 * ] as const satisfies MpvObservableProperty[];
 *
 * // Observe properties
 * const unlisten = await observeProperties(
 *   OBSERVED_PROPERTIES,
 *   ({ name, data }) => {
 *     switch (name) {
 *       case 'pause':
 *         console.log('Playback paused state:', data);
 *         break;
 *       // data type: number | null
 *       case 'time-pos':
 *         console.log('Current time position:', data);
 *         break;
 *       // data type: number | null
 *       case 'duration':
 *         console.log('Duration:', data);
 *         break;
 *       // data type: string | null
 *       case 'filename':
 *         console.log('Current playing file:', data);
 *         break;
 *     }
 *   });
 *
 * // Unlisten when no longer needed
 * unlisten();
 * ```
 */
export async function observeProperties(properties, callback, windowLabel) {
    const propertyNames = properties.map((p) => p[0]);
    return await listenEvents((mpvEvent) => {
        if (mpvEvent.event === "property-change") {
            if (mpvEvent.name && propertyNames.includes(mpvEvent.name)) {
                callback(mpvEvent);
            }
        }
    }, windowLabel);
}

/**
 * Listen to all mpv events.
 *
 * @param {(event: MpvEvent) => void} callback - Function to call when mpv events are received
 * @param {string} [windowLabel] - Target window label, defaults to current window
 * @returns {Promise<UnlistenFn>} Function to call to stop listening
 *
 * @example
 * ```typescript
 * import { listenEvents } from 'tauri-plugin-libmpv-api';
 *
 * const unlisten = await listenEvents((event) => {
 *     console.log(event);
 * });
 *
 * // Unlisten when no longer needed
 * unlisten();
 * ```
 */
export async function listenEvents(callback, windowLabel) {
    if (!windowLabel) {
        windowLabel = getCurrentWindow().label;
    }
    const eventName = `mpv-event-${windowLabel}`;
    return await listen(eventName, (event) => callback(event.payload));
}

/**
 * Send mpv command
 *
 * @param {string} name - Command name
 * @param {Array<string | boolean | number>} [args] - Command arguments
 * @param {string} [windowLabel] - Target window label, defaults to current window.
 * @throws {Error} Throws an error if the command fails.
 *
 * @see {@link https://mpv.io/manual/master/#list-of-input-commands} for a full list of commands.
 *
 * @example
 * ```typescript
 * import { command } from 'tauri-plugin-libmpv-api';
 *
 * // Load file
 * await command('loadfile', ['/path/to/video.mp4']);
 *
 * // Play/pause
 * await command('set', ['pause', false]);
 * await command('set', ['pause', true]);
 *
 * // Seek to position
 * await command('seek', [30, 'absolute']);
 * await command('seek', [10, 'relative']);
 *
 * // Set volume
 * await command('set', ['volume', 80]);
 *
 * ```
 */
export async function command(name, args = [], windowLabel) {
    if (!windowLabel) {
        windowLabel = getCurrentWindow().label;
    }
    await invoke("plugin:libmpv|command", {
        name,
        args,
        windowLabel,
    });
}

/**
 * Set mpv property
 *
 * @param {string} name - Property name
 * @param {string | boolean | number} value - Property value
 * @param {string} [windowLabel] - Target window label, defaults to current window.
 * @throws {Error} Throws an error if the command fails.
 *
 * @see {@link https://mpv.io/manual/master/#properties} for a full list of properties.
 *
 * @example
 * ```typescript
 * import { setProperty } from 'tauri-plugin-libmpv-api';
 *
 * // Play/pause
 * await setProperty('pause', false);
 * await setProperty('pause', true);
 *
 * // Set volume
 * await setProperty('volume', 80);
 *
 * ```
 */
export async function setProperty(name, value, windowLabel) {
    if (!windowLabel) {
        windowLabel = getCurrentWindow().label;
    }
    await invoke("plugin:libmpv|set_property", {
        name,
        value,
        windowLabel,
    });
}

export async function getProperty(name, format, windowLabel) {
    if (!windowLabel) {
        windowLabel = getCurrentWindow().label;
    }
    return await invoke("plugin:libmpv|get_property", {
        name,
        format,
        windowLabel,
    });
}

/**
 * Set video margin ratio
 * @param {VideoMarginRatio} ratio - Margin ratio configuration object
 * @param {string} [windowLabel] - Target window label, defaults to current window
 * @returns {Promise<void>} Promise with no return value
 * @throws {Error} Throws error when setting fails
 *
 * @example
 * ```typescript
 * import { setVideoMarginRatio } from 'tauri-plugin-libmpv-api';
 *
 * // Leave 10% space at bottom for control bar
 * await setVideoMarginRatio({ bottom: 0.1 });
 *
 * // Leave margins on all sides
 * await setVideoMarginRatio({
 *   left: 0.05,
 *   right: 0.05,
 *   top: 0.1,
 *   bottom: 0.15
 * });
 *
 * // Reset margins (remove all margins)
 * await setVideoMarginRatio({
 *   left: 0,
 *   right: 0,
 *   top: 0,
 *   bottom: 0
 * });
 * ```
 */
export async function setVideoMarginRatio(ratio, windowLabel) {
    if (!windowLabel) {
        const currentWindow = getCurrentWindow();
        windowLabel = currentWindow.label;
    }
    return await invoke("plugin:libmpv|set_video_margin_ratio", {
        ratio,
        windowLabel,
    });
}
