export function formatTime(seconds) {
  if (seconds === null || seconds === undefined || isNaN(seconds)) {
    return "00:00";
  }
  const flooredSeconds = Math.floor(seconds);
  const h = Math.floor(flooredSeconds / 3600);
  const m = Math.floor((flooredSeconds % 3600) / 60);
  const s = flooredSeconds % 60;
  if (h <= 0) {
    return `${m < 10 ? "0" : ""}${m}:${s < 10 ? "0" : ""}${s}`;
  } else {
    return `${h}:${m < 10 ? "0" : ""}${m}:${s < 10 ? "0" : ""}${s}`;
  }
}
