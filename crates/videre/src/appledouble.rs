//! Recognize AppleDouble metadata without guessing from a `._` filename.

/// Fixed header size in AppleSingle/AppleDouble versions 1 and 2.
pub const HEADER_LEN: usize = 26;

/// Recognize the AppleDouble magic, version and fixed header, and check that
/// the declared entry table fits in the file. This identifies the container;
/// it does not validate or read its resource-fork contents. AppleSingle has a
/// different magic and can contain the original data, so it is never skipped.
pub fn is_header(head: &[u8], file_len: u64) -> bool {
    if head.len() < HEADER_LEN || !head.starts_with(b"\x00\x05\x16\x07") {
        return false;
    }
    if !matches!(&head[4..8], b"\x00\x01\x00\x00" | b"\x00\x02\x00\x00") {
        return false;
    }
    let entries = u16::from_be_bytes([head[24], head[25]]);
    HEADER_LEN as u64 + u64::from(entries) * 12 <= file_len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(version: u32, entries: u16) -> [u8; HEADER_LEN] {
        let mut head = [0; HEADER_LEN];
        head[..4].copy_from_slice(&0x0005_1607u32.to_be_bytes());
        head[4..8].copy_from_slice(&version.to_be_bytes());
        head[24..].copy_from_slice(&entries.to_be_bytes());
        head
    }

    #[test]
    fn appledouble_versions_are_recognized_without_reading_the_payload() {
        for version in [0x0001_0000, 0x0002_0000] {
            assert!(is_header(&header(version, 1), 70));
            assert!(is_header(&header(version, 0), HEADER_LEN as u64));
        }
    }

    #[test]
    fn appledouble_detection_preserves_other_formats_and_uncertain_headers() {
        let good = header(0x0002_0000, 1);
        for length in 0..HEADER_LEN {
            assert!(!is_header(&good[..length], 70));
        }
        assert!(!is_header(&good, 37), "the entry table does not fit");
        assert!(!is_header(&header(0x0003_0000, 0), 70));
        let mut single = good;
        single[..4].copy_from_slice(&0x0005_1600u32.to_be_bytes());
        assert!(!is_header(&single, 70));
        let mut jpeg = good;
        jpeg[..3].copy_from_slice(b"\xff\xd8\xff");
        assert!(!is_header(&jpeg, 70));
    }
}
