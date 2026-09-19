//! Content hashing.
//!
//! The snapshot store is content-addressed, and crash recovery decides whether an
//! operation ran by asking whether the file on disk hashes to the plan's *before*
//! or *after* value. That makes the hash the engine's reference point rather than
//! an implementation detail.

use std::fmt;
use std::io::{self, Read};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::dataformatting::Msg;
use crate::error::{Denial, ErrorCode};

/// Streaming buffer size.
const READ_BUFFER: usize = 64 * 1024;

/// Characters in the hexadecimal form.
const HEX_LEN: usize = 64;

/// A BLAKE3 content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hashes a buffer already in memory.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Hashes a stream, returning the hash and the number of bytes read.
    ///
    /// Used for files, which should not be required to fit in memory.
    pub fn of_reader<R: Read>(mut reader: R) -> io::Result<(Self, u64)> {
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; READ_BUFFER];
        let mut len = 0u64;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            len += read as u64;
        }
        Ok((Self(*hasher.finalize().as_bytes()), len))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        use fmt::Write as _;
        let mut out = String::with_capacity(HEX_LEN);
        for byte in self.0 {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    pub fn from_hex(text: &str) -> Result<Self, Denial> {
        if text.len() != HEX_LEN {
            return Err(malformed(Msg::HashBadLength { len: text.len() }));
        }
        let mut out = [0u8; 32];
        for (index, slot) in out.iter_mut().enumerate() {
            let pair = &text[index * 2..index * 2 + 2];
            *slot = u8::from_str_radix(pair, 16).map_err(|_| {
                malformed(Msg::HashNotHexadecimal {
                    text: pair.to_owned(),
                })
            })?;
        }
        Ok(Self(out))
    }

    /// Shard directory name for the snapshot store, which keeps a single
    /// directory from accumulating tens of thousands of entries. Snapshots live
    /// at `snapshots/<shard>/<hex>`.
    pub fn shard(self) -> String {
        format!("{:02x}", self.0[0])
    }

    /// A short form for display.
    pub fn short(self) -> String {
        self.to_hex()[..12].to_owned()
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Abbreviated so logs are not flooded with full digests.
impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.short())
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        ContentHash::from_hex(&raw).map_err(serde::de::Error::custom)
    }
}

/// A malformed digest means a damaged record or a hand-edited file, which a
/// person has to look at; retrying achieves nothing.
fn malformed(message: Msg) -> Denial {
    Denial::new(ErrorCode::InvalidHash, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_hashes_identically() {
        assert_eq!(
            ContentHash::of_bytes(b"hello"),
            ContentHash::of_bytes(b"hello")
        );
        assert_ne!(
            ContentHash::of_bytes(b"hello"),
            ContentHash::of_bytes(b"hellp")
        );
    }

    #[test]
    fn streaming_matches_in_memory_hashing() {
        let data = vec![7u8; READ_BUFFER * 3 + 17];
        let (streamed, len) = ContentHash::of_reader(&data[..]).unwrap();
        assert_eq!(len, data.len() as u64);
        assert_eq!(streamed, ContentHash::of_bytes(&data));
    }

    #[test]
    fn handles_empty_input() {
        let (hash, len) = ContentHash::of_reader(&b""[..]).unwrap();
        assert_eq!(len, 0);
        assert_eq!(hash, ContentHash::of_bytes(b""));
    }

    #[test]
    fn hexadecimal_round_trips() {
        let hash = ContentHash::of_bytes(b"safescope");
        let hex = hash.to_hex();
        assert_eq!(hex.len(), HEX_LEN);
        assert_eq!(ContentHash::from_hex(&hex).unwrap(), hash);
    }

    #[test]
    fn rejects_malformed_hexadecimal() {
        assert!(ContentHash::from_hex("abc").is_err());
        assert!(ContentHash::from_hex(&"z".repeat(HEX_LEN)).is_err());
    }

    #[test]
    fn shard_is_the_first_hex_byte() {
        let hash = ContentHash::of_bytes(b"x");
        let shard = hash.shard();
        assert_eq!(shard.len(), 2);
        assert!(hash.to_hex().starts_with(&shard));
    }

    #[test]
    fn json_round_trips() {
        let hash = ContentHash::of_bytes(b"payload");
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(serde_json::from_str::<ContentHash>(&json).unwrap(), hash);
    }
}
