export function isVideoFile(path) {
  const videoExtensions = [".mp4", ".webm", ".ogg", ".mov", ".avi", ".mkv", ".flv", ".wmv", ".m4v"];

  const lowerPath = path.toLowerCase();
  return videoExtensions.some((ext) => lowerPath.endsWith(ext));
}
