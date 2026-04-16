// Plex TV show naming convention parser
// Supports: "Show Name - S01E02 - Episode Title.mkv"
//           "Show Name s01e02 Episode Title.mkv"
//           "Show.Name.S01E02.Episode.Title.mkv"
//           "Show Name (2008) - S01E02-E03 - Episode Title.mkv"

const TV_SHOW_REGEX =
    /^(?<show>.+?)\s*[-.]?\s*[Ss](?<season>\d{1,2})\s*[Ee](?<episode>\d{1,2})(?:\s*[-.]?\s*[Ee](?<episodeEnd>\d{1,2}))?\s*[-.]?\s*(?<title>.*?)$/;

// prettier-ignore
const METADATA_TAGS = new Set([
    // Resolution
    "480p", "576p", "720p", "1080p", "1080i", "2160p", "4k",
    // Source
    "bluray", "bdrip", "brrip", "webrip", "web-dl", "webdl", "web", "hdtv",
    "dvdrip", "hdrip", "hdcam", "cam", "ts", "telesync", "amzn", "nf",
    "dsnp", "hmax", "atvp", "pcok", "hulu", "cr", "it",
    // Video codec
    "x264", "x265", "h264", "h265", "h.264", "h.265", "hevc",
    "avc", "av1", "vp9", "mpeg2", "xvid", "divx",
    // HDR
    "hdr", "hdr10", "hdr10+", "hdr10plus", "dolby vision", "dv", "sdr",
    // Audio codec
    "aac", "aac5", "ac3", "eac3", "dts", "dts-hd", "dtshd",
    "flac", /*"opus",*/ "truehd", "atmos", "ddp5", "ddp", "dd5", "dd",
    "lpcm", "mp3",
    // Audio channels
    // "2.0", "5.1", "7.1",
    // Release group / misc
    // "remux", "proper", "repack", "internal", "limited", "extended",
    // "uncut", "remastered", "dubbed", "multi", "dual", "10bit", "8bit",
    // "ntb", "ctl", "sigma", "yts", "rarbg", "sparks", "geckos",
]);

/**
 * @param {string} filename
 * @returns {{ show: string, season: number, episode: number, episodeEnd?: number, title?: string } | null}
 */
export function parseTvShow(filename) {
    if (!filename) return null;

    // Strip file extension
    const dot = filename.lastIndexOf(".");
    const name = dot > 0 ? filename.substring(0, dot) : filename;

    const match = name.match(TV_SHOW_REGEX);
    if (!match) return null;

    const show = cleanSeparators(match.groups.show);
    const season = parseInt(match.groups.season, 10);
    const episode = parseInt(match.groups.episode, 10);
    const episodeEnd = match.groups.episodeEnd ? parseInt(match.groups.episodeEnd, 10) : undefined;
    const title = stripMetadata(cleanSeparators(match.groups.title)) || undefined;

    return { show, season, episode, episodeEnd, title };
}

/** Replace dots/underscores used as word separators and trim surrounding junk. */
export function cleanSeparators(str) {
    return str.replace(/[._]/g, " ").replace(/\s+/g, " ").trim();
}

/** Truncate at the first word that matches a known metadata tag. */
export function stripMetadata(str) {
    const words = str.split(" ");
    for (let i = 0; i < words.length; i++) {
        const bare = words[i].replace(/[()[\]{}]/g, "").toLowerCase();
        if (bare && METADATA_TAGS.has(bare)) {
            return words
                .slice(0, i)
                .join(" ")
                .replace(/[-\s(]+$/, "");
        }
    }
    return str;
}
