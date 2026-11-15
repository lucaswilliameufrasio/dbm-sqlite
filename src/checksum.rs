use sha2::{Digest, Sha256};

pub(crate) fn raw_checksum(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    format!("sha256:raw:v1:{encoded}")
}

#[cfg(test)]
mod tests {
    use super::raw_checksum;

    #[test]
    fn checksum_is_sha256_of_the_exact_bytes() {
        let checksum = raw_checksum(b"abc");
        assert_eq!(
            checksum,
            "sha256:raw:v1:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
