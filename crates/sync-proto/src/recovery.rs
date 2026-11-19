//! The recovery key: the account key in a form people can write down
//! (design §5.1). Crockford base32 in groups of four, with a short checksum so
//! typos are caught before trying to decrypt anything.
//!
//! `FT1-ABCD-EFGH-…` — case, spaces and dashes don't matter; `O`/`I`/`L` are
//! read as `0`/`1`/`1`.

use crate::crypto::AccountKey;
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const PREFIX: &str = "FT1";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.chars() {
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        let v = ALPHABET.iter().position(|&a| a as char == c)? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn checksum(key: &[u8]) -> String {
    encode(&Sha256::digest(key)[..2])[..3].to_string()
}

/// `FT1-XXXX-XXXX-…-CCC` (13 groups of key, then a 3-character checksum).
pub fn format(key: &AccountKey) -> String {
    let body = encode(key.bytes());
    let mut groups: Vec<String> = body
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    groups.insert(0, PREFIX.into());
    groups.push(checksum(key.bytes()));
    groups.join("-")
}

/// Parses a recovery key typed by the user.
pub fn parse(input: &str) -> Result<AccountKey, &'static str> {
    let clean: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let rest = clean
        .strip_prefix(PREFIX)
        .ok_or("A recovery key starts with FT1")?;
    if rest.len() != 52 + 3 {
        return Err("That recovery key is the wrong length");
    }
    // Read look-alike letters as the digits they resemble.
    let rest: String = rest
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect();
    let (body, check) = rest.split_at(52);
    let bytes = decode(body).ok_or("That recovery key has characters that don't belong")?;
    let arr: [u8; 32] = bytes
        .get(..32)
        .and_then(|b| b.try_into().ok())
        .ok_or("That recovery key is the wrong length")?;
    if checksum(&arr) != check {
        return Err("That recovery key has a typo (the check doesn't match)");
    }
    Ok(AccountKey::from_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_forgiving_input() {
        let key = AccountKey::generate();
        let text = format(&key);
        assert!(text.starts_with("FT1-"));
        assert_eq!(parse(&text).unwrap().bytes(), key.bytes());
        // Lower case, spaces instead of dashes.
        let sloppy = text.to_lowercase().replace('-', " ");
        assert_eq!(parse(&sloppy).unwrap().bytes(), key.bytes());
    }

    #[test]
    fn typos_are_caught() {
        let text = format(&AccountKey::generate());
        let mut chars: Vec<char> = text.chars().collect();
        let i = 6; // inside the first key group
        chars[i] = if chars[i] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        assert!(parse(&typo).is_err());
        assert!(parse("FT1-ABCD").is_err());
        assert!(parse("hello").is_err());
    }
}
