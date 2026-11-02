# Keys

Keys the player is built with, one file each, read by `build.rs`. Nothing in
this folder but this note is committed — see `.gitignore` beside it.

* `opensubtitles` — the API key for OpenSubtitles, registered free for the
  player at <https://www.opensubtitles.com/consumers>. Without it the player
  builds and runs as before, with no *Find subtitles* row. The environment
  variable `DBM_OPENSUBTITLES_KEY` does the same and wins over the file;
  CI's Windows build sets it from the repository secret `OPENSUBTITLES`.

The file is the key and nothing else; surrounding whitespace is ignored.
