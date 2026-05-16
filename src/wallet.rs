use std::fs;
use std::path::Path;
use rand::RngCore;

const WALLET_FILE: &str = "ewatts_data/wallet.json";

pub fn generate_key() -> ([u8; 32], Vec<u8>) {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let sk = ed25519_dalek::SigningKey::from_bytes(&secret);
    let pubkey = sk.verifying_key().to_bytes().to_vec();
    (secret, pubkey)
}

pub fn save_key(secret: &[u8; 32], pubkey: &[u8]) {
    let mut keys = load_keys();
    keys.push((*secret, pubkey.to_vec()));
    let json = serde_json::to_string(&keys).unwrap();
    fs::create_dir_all("ewatts_data").ok();
    fs::write(WALLET_FILE, &json).ok();
}

pub fn load_keys() -> Vec<([u8; 32], Vec<u8>)> {
    if !Path::new(WALLET_FILE).exists() { return vec![]; }
    let data = fs::read_to_string(WALLET_FILE).unwrap_or_default();
    serde_json::from_str(&data).unwrap_or_default()
}
