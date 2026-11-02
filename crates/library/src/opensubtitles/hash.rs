use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// OpenSubtitles' hash of a file: its size plus the first and last 64 KiB
/// read as little-endian 64-bit words, all added with wrapping. Sixteen hex
/// digits. Cheap (two reads), and the same for every copy of a release.
pub(super) fn hash(path: &Path) -> Option<String> {
    const CHUNK: u64 = 64 * 1024;
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < CHUNK {
        return None;
    }
    let mut sum = size;
    let mut buf = vec![0u8; CHUNK as usize];
    for at in [0, size - CHUNK] {
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut buf).ok()?;
        for word in buf.as_chunks::<8>().0 {
            sum = sum.wrapping_add(u64::from_le_bytes(*word));
        }
    }
    Some(format!("{sum:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_size_and_both_ends_added_up() {
        let file = std::env::temp_dir().join("dbm-osdb-hash-test.bin");
        // 128 KiB of ones: 16384 words of 0x0101..01 at each end, plus the size.
        std::fs::write(&file, vec![1u8; 128 * 1024]).unwrap();
        let word = u64::from_le_bytes([1; 8]);
        let expected = (128u64 * 1024)
            .wrapping_add(word.wrapping_mul(8192))
            .wrapping_add(word.wrapping_mul(8192));
        assert_eq!(hash(&file), Some(format!("{expected:016x}")));
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_file_too_small_to_hash_has_none() {
        let file = std::env::temp_dir().join("dbm-osdb-small-test.bin");
        std::fs::write(&file, b"short").unwrap();
        assert_eq!(hash(&file), None);
        std::fs::remove_file(&file).ok();
    }
}
