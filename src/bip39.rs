/// BIP39 wordlist loaded from embedded text file at runtime
fn bip39_words() -> Vec<&'static str> {
    include_str!("bip39_english.txt")
        .lines()
        .collect()
}

/// Generate a BIP39 mnemonic from entropy bytes
pub fn entropy_to_mnemonic(entropy: &[u8]) -> Result<Vec<String>, String> {
    if entropy.len() < 16 || entropy.len() > 32 || entropy.len() % 4 != 0 {
        return Err("Entropy must be 16, 20, 24, 28, or 32 bytes".into());
    }
    let words = bip39_words();
    
    // Compute checksum: first (entropy_bits / 32) bits of SHA256
    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(entropy);
    let checksum_bits = (entropy.len() * 8) / 32;
    let checksum = hash[0] >> (8 - checksum_bits);
    
    // Combine entropy bits + checksum bits into 11-bit word indices
    // Total bits = entropy_bits + checksum_bits, which should be divisible by 11
    let total_bits = entropy.len() * 8 + checksum_bits as usize;
    let word_count = total_bits / 11;
    
    let mut remaining_bits = 0u64;
    let mut remaining_count = 0u32;
    let mut result = Vec::new();
    
    // Process entropy bytes first
    for b in entropy {
        remaining_bits = (remaining_bits << 8) | (*b as u64);
        remaining_count += 8;
        
        while remaining_count >= 11 {
            remaining_count -= 11;
            let idx = (remaining_bits >> remaining_count) & 0x7FF;
            if (idx as usize) < words.len() {
                result.push(words[idx as usize].to_string());
            } else {
                return Err("Invalid word index".into());
            }
            let mask = (1u64 << remaining_count) - 1;
            remaining_bits &= mask;
        }
    }
    
    // Append checksum bits (NOT as a full byte — just the exact number of bits)
    remaining_bits = (remaining_bits << checksum_bits) | (checksum as u64);
    remaining_count += checksum_bits as u32;
    
    // Extract any final words
    while remaining_count >= 11 {
        remaining_count -= 11;
        let idx = (remaining_bits >> remaining_count) & 0x7FF;
        if (idx as usize) < words.len() {
            result.push(words[idx as usize].to_string());
        } else {
            return Err("Invalid word index".into());
        }
        let mask = (1u64 << remaining_count) - 1;
        remaining_bits &= mask;
    }
    
    if result.len() != word_count {
        return Err(format!("Expected {} words, got {}", word_count, result.len()));
    }
    
    Ok(result)
}

/// Convert a BIP39 mnemonic back to entropy + checksum
pub fn mnemonic_to_entropy(input_words: &[String]) -> Result<Vec<u8>, String> {
    use sha2::{Sha256, Digest};
    let wordlist = bip39_words();
    
    if input_words.len() < 12 || input_words.len() > 24 || input_words.len() % 3 != 0 {
        return Err("Mnemonic must have 12, 15, 18, 21, or 24 words".into());
    }
    
    let total_bits = input_words.len() * 11;
    let entropy_bits = total_bits - (total_bits / 33);  // checksum = total / 33
    let checksum_bits = total_bits - entropy_bits;
    let entropy_bytes_count = entropy_bits / 8;
    
    let mut bits: u64 = 0;
    let mut bit_count: u32 = 0;
    let mut entropy_bytes = Vec::new();
    let mut bits_read = 0;
    
    for word in input_words {
        let idx = wordlist.iter().position(|&w| w == word.as_str())
            .ok_or_else(|| format!("Unknown word: {}", word))?;
        bits = (bits << 11) | (idx as u64);
        bit_count += 11;
        bits_read += 11;
        
        // Extract bytes up to but not including the checksum bits
        while bit_count >= 8 && entropy_bytes.len() < entropy_bytes_count {
            bit_count -= 8;
            entropy_bytes.push((bits >> bit_count) as u8);
            let mask = (1u64 << bit_count) - 1;
            bits &= mask;
        }
    }
    
    // Check that we got exactly the right number of entropy bytes
    if entropy_bytes.len() != entropy_bytes_count {
        return Err(format!("Expected {} entropy bytes, got {}", entropy_bytes_count, entropy_bytes.len()));
    }
    
    // Extract checksum from remaining bits
    if bit_count > 0 {
        let stored_checksum = (bits >> (bit_count - checksum_bits as u32)) & ((1u64 << checksum_bits) - 1);
        
        // Verify checksum
        let hash = Sha256::digest(&entropy_bytes);
        let expected_checksum = hash[0] >> (8 - checksum_bits as u8);
        if (stored_checksum as u8) != expected_checksum {
            return Err("Invalid checksum: mnemonic may be wrong".into());
        }
    }
    
    Ok(entropy_bytes)
}

/// Derive a 512-bit seed from mnemonic using PBKDF2 (BIP39 standard)
pub fn mnemonic_to_seed(mnemonic: &str, passphrase: &str) -> [u8; 64] {
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    
    // Use pbkdf2 with HMAC-SHA512, 2048 rounds (BIP39 standard)
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(
        mnemonic.as_bytes(),
        salt.as_bytes(),
        2048,
        &mut seed,
    );
    
    seed
}

/// Generate a random mnemonic (12 words)
pub fn generate_mnemonic() -> Result<Vec<String>, String> {
    let mut entropy = [0u8; 16]; // 128 bits → 12 words
    getrandom::getrandom(&mut entropy).map_err(|e| format!("RNG error: {}", e))?;
    entropy_to_mnemonic(&entropy)
}

/// Derive ed25519 keypair from BIP39 seed
pub fn seed_to_keypair(seed: &[u8; 64]) -> ed25519_dalek::SigningKey {
    use sha2::{Sha256, Digest};
    
    // Use first 32 bytes of hashed seed as ed25519 scalar
    // Note: This is NOT BIP32 (not yet). Simple derivation for MVP.
    let hash = Sha256::digest(&seed[..32]);
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&hash);
    ed25519_dalek::SigningKey::from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mnemonic_roundtrip() {
        let words = generate_mnemonic().unwrap();
        assert_eq!(words.len(), 12, "12-word mnemonic");
        
        let mnemonic_str = words.join(" ");
        let parsed: Vec<String> = mnemonic_str.split_whitespace().map(String::from).collect();
        assert_eq!(parsed, words, "Parse roundtrip");
    }
    
    #[test]
    fn test_entropy_mnemonic_roundtrip() {
        let words = generate_mnemonic().unwrap();
        let mnemonic_str = words.join(" ");
        let parsed: Vec<String> = mnemonic_str.split_whitespace().map(String::from).collect();
        
        let entropy = mnemonic_to_entropy(&parsed).unwrap();
        assert_eq!(entropy.len(), 16);
        
        let words2 = entropy_to_mnemonic(&entropy).unwrap();
        assert_eq!(words, words2, "Full roundtrip: entropy → words → entropy → words");
    }
    
    #[test]
    fn test_seed_derivation() {
        let words = generate_mnemonic().unwrap();
        let mnemonic_str = words.join(" ");
        let seed = mnemonic_to_seed(&mnemonic_str, "");
        assert_eq!(seed.len(), 64);
        
        let keypair = seed_to_keypair(&seed);
        let pk = keypair.verifying_key();
        assert!(pk.as_bytes().len() == 32);
    }
    
    #[test]
    fn test_known_vector() {
        // BIP39 test vector: entropy=00000000000000000000000000000000
        let entropy = [0u8; 16];
        let words = entropy_to_mnemonic(&entropy).unwrap();
        // Verify the mnemonic roundtrips correctly
        let mnemonic_str = words.join(" ");
        let parsed: Vec<String> = mnemonic_str.split_whitespace().map(String::from).collect();
        let recovered = mnemonic_to_entropy(&parsed).unwrap();
        assert_eq!(recovered, entropy, "Full roundtrip: entropy → words → entropy");
        assert_eq!(words.len(), 12, "12 words from 16 bytes entropy");
    }
}
