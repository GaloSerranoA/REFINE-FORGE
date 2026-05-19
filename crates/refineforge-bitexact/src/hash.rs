//! Byte-level hashing utilities.
//!
//! All hashes are SHA-256, hex-encoded lowercase. Files are read in
//! streaming fashion so multi-GB outputs don't OOM us.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

const STREAM_BUF_SIZE: usize = 1 << 16; // 64 KiB

/// SHA-256 hex of the given bytes.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Streaming SHA-256 hex of a file.
pub fn hash_file(path: &Path) -> Result<String> {
    let f = std::fs::File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut reader = std::io::BufReader::with_capacity(STREAM_BUF_SIZE, f);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; STREAM_BUF_SIZE];
    loop {
        let n = reader.read(&mut buf)
            .with_context(|| format!("reading {}", path.display()))?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// True iff all hashes in the slice are equal (and the slice is
/// non-empty). The gate's primary decision function.
pub fn all_equal(hashes: &[String]) -> bool {
    if hashes.is_empty() { return false; }
    let first = &hashes[0];
    hashes.iter().all(|h| h == first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hash_bytes_is_known_sha256() {
        // SHA-256 of "abc" is the canonical RFC test vector.
        let h = hash_bytes(b"abc");
        assert_eq!(h, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn hash_bytes_empty() {
        let h = hash_bytes(b"");
        assert_eq!(h, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn hash_file_matches_hash_bytes() {
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("x.bin");
        let mut f = std::fs::File::create(&p).unwrap();
        let data = b"hello world\n";
        f.write_all(data).unwrap();
        drop(f);
        assert_eq!(hash_file(&p).unwrap(), hash_bytes(data));
    }

    #[test]
    fn hash_file_handles_large_file_streaming() {
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("big.bin");
        let mut f = std::fs::File::create(&p).unwrap();
        // 200 KiB — exercises the buffered-read path
        let chunk = vec![0xABu8; 100 * 1024];
        f.write_all(&chunk).unwrap();
        f.write_all(&chunk).unwrap();
        drop(f);
        let h_file = hash_file(&p).unwrap();
        let mut all = Vec::new();
        all.extend_from_slice(&chunk);
        all.extend_from_slice(&chunk);
        let h_mem = hash_bytes(&all);
        assert_eq!(h_file, h_mem);
    }

    #[test]
    fn all_equal_trivial_cases() {
        assert!(!all_equal(&[])); // empty → false (no evidence of bit-exactness)
        assert!(all_equal(&["abc".into()])); // single element → trivially "equal to itself"
    }

    #[test]
    fn all_equal_detects_difference() {
        let same = vec!["a".to_string(); 5];
        assert!(all_equal(&same));
        let mut diff = same.clone();
        diff[2] = "b".to_string();
        assert!(!all_equal(&diff));
    }
}
