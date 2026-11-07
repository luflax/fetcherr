//! Resolves the v1 info-hash of a torrent so the download id is known before qBittorrent sees it.

use sha1::{Digest, Sha1};

use crate::domain::DownloadId;

#[derive(Debug, Clone, PartialEq)]
pub enum TorrentPayload {
    Magnet(String),
    File(Vec<u8>),
}

impl TorrentPayload {
    pub fn info_hash(&self) -> Result<DownloadId, String> {
        match self {
            Self::Magnet(uri) => info_hash_from_magnet(uri),
            Self::File(bytes) => info_hash_from_torrent(bytes),
        }
    }
}

pub fn info_hash_from_magnet(uri: &str) -> Result<DownloadId, String> {
    let query = uri
        .strip_prefix("magnet:?")
        .ok_or_else(|| "not a magnet link".to_string())?;
    let btih = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| {
            let is_exact_topic = key == "xt" || key.starts_with("xt.");
            is_exact_topic
                .then(|| value.strip_prefix("urn:btih:"))
                .flatten()
        })
        .ok_or_else(|| "magnet link has no v1 info-hash (urn:btih)".to_string())?;

    let hex = match btih.len() {
        40 => btih.to_string(),
        32 => hex::encode(decode_base32(btih)?),
        _ => return Err(format!("invalid info-hash '{btih}'")),
    };
    DownloadId::try_from(hex).map_err(|err| err.to_string())
}

pub fn info_hash_from_torrent(bytes: &[u8]) -> Result<DownloadId, String> {
    let info = info_dict_span(bytes)?;
    let digest = Sha1::digest(info);
    DownloadId::try_from(hex::encode(digest)).map_err(|err| err.to_string())
}

/// Returns the raw bencoded bytes of the top-level `info` dictionary.
fn info_dict_span(bytes: &[u8]) -> Result<&[u8], String> {
    let mut cursor = Bencode { bytes, pos: 0 };
    cursor.expect(b'd')?;
    while cursor.peek()? != b'e' {
        let key = cursor.byte_string()?;
        let value_start = cursor.pos;
        cursor.skip_value()?;
        if key == b"info" {
            return Ok(&bytes[value_start..cursor.pos]);
        }
    }
    Err("torrent file has no info dictionary".to_string())
}

struct Bencode<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Bencode<'a> {
    fn peek(&self) -> Result<u8, String> {
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or_else(|| "truncated torrent file".to_string())
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek()? != byte {
            return Err(format!("malformed torrent file at byte {}", self.pos));
        }
        self.pos += 1;
        Ok(())
    }

    fn skip_value(&mut self) -> Result<(), String> {
        match self.peek()? {
            b'i' => self.skip_until_end(),
            b'l' | b'd' => {
                self.pos += 1;
                while self.peek()? != b'e' {
                    self.skip_value()?;
                }
                self.pos += 1;
                Ok(())
            }
            b'0'..=b'9' => self.byte_string().map(|_| ()),
            other => Err(format!(
                "unexpected byte '{}' in torrent file",
                other as char
            )),
        }
    }

    fn skip_until_end(&mut self) -> Result<(), String> {
        while self.peek()? != b'e' {
            self.pos += 1;
        }
        self.pos += 1;
        Ok(())
    }

    fn byte_string(&mut self) -> Result<&'a [u8], String> {
        let colon = self.bytes[self.pos..]
            .iter()
            .position(|&b| b == b':')
            .ok_or_else(|| "truncated torrent file".to_string())?;
        let length: usize = std::str::from_utf8(&self.bytes[self.pos..self.pos + colon])
            .ok()
            .and_then(|digits| digits.parse().ok())
            .ok_or_else(|| "invalid string length in torrent file".to_string())?;
        let start = self.pos + colon + 1;
        let end = start
            .checked_add(length)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| "truncated torrent file".to_string())?;
        self.pos = end;
        Ok(&self.bytes[start..end])
    }
}

fn decode_base32(input: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut output = Vec::with_capacity(input.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for symbol in input.bytes().map(|b| b.to_ascii_uppercase()) {
        let value = ALPHABET
            .iter()
            .position(|&a| a == symbol)
            .ok_or_else(|| format!("invalid base32 info-hash '{input}'"))?;
        buffer = (buffer << 5) | value as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEBIAN_HASH: &str = "8c212779b4abde7c6bc608063a0d008b7e40ce32";

    #[test]
    fn reads_hex_info_hash_from_magnet() {
        let magnet = format!(
            "magnet:?xt=urn:btih:{}&dn=debian",
            DEBIAN_HASH.to_uppercase()
        );
        assert_eq!(
            info_hash_from_magnet(&magnet).unwrap().as_str(),
            DEBIAN_HASH
        );
    }

    #[test]
    fn reads_base32_info_hash_from_magnet() {
        let magnet = "magnet:?xt=urn:btih:RQQSO6NUVPPHY26GBADDUDIARN7EBTRS";
        assert_eq!(info_hash_from_magnet(magnet).unwrap().as_str(), DEBIAN_HASH);
    }

    #[test]
    fn rejects_magnet_without_btih() {
        assert!(info_hash_from_magnet("magnet:?dn=nothing").is_err());
    }

    #[test]
    fn hashes_only_the_info_dictionary() {
        let info = b"d6:lengthi42e4:name4:test12:piece lengthi16384e6:pieces0:e";
        let torrent = [
            b"d8:announce14:http://tracker4:info".as_slice(),
            info.as_slice(),
            b"e".as_slice(),
        ]
        .concat();
        let expected = hex::encode(Sha1::digest(info));
        assert_eq!(info_hash_from_torrent(&torrent).unwrap().as_str(), expected);
    }

    #[test]
    fn rejects_garbage() {
        assert!(info_hash_from_torrent(b"<html>login</html>").is_err());
    }
}
