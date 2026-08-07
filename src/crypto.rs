use argon2::{
    password_hash::{
        PasswordHasher,
        SaltString
    },
    Argon2
};
use chacha20poly1305::{
    aead::{
        Aead,
        AeadCore,
        KeyInit
    },
    ChaCha20Poly1305,
    Nonce
};
use rand::rngs::OsRng;
use rand::RngCore;
use std::io::{Error, ErrorKind};

const SL: usize = 16;    
const NL: usize = 12;   

fn other_error(msg: &str) -> Error {
    Error::new(ErrorKind::Other, msg)
}

fn invalid_data_error(msg: &str) -> Error {
    Error::new(ErrorKind::InvalidData, msg)
}

/// encrypt with passphrase
/// Fmt : [salt(16bytes)] + [nonce(12bytes)] + [ciphertext]
pub fn encrypt(
    plaintext: &[u8],
    passphrase: &str
) -> Result<Vec<u8>, Error> {
    // - gen salt (16 bytes random)
    let mut sb = [0u8; SL];
    OsRng.fill_bytes(&mut sb);
    let salt = SaltString::encode_b64(&sb)
        .map_err(|_| other_error("Salt encoding failed"))?;

    // - derive key with Argon2id
    let ag2 = Argon2::default();
    let pass_hash = ag2
        .hash_password(passphrase.as_bytes(), &salt)
        .map_err(|_| other_error("Argon2 hashing failed"))?;
    let hash = pass_hash.hash.unwrap();
    let key_bytes: [u8; 32] = hash.as_bytes()[..32]
        .try_into()
        .map_err(|_| other_error("Key derivation failed"))?;

    // - gen nonce
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    // - encrypt
    let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
        .map_err(|_| other_error("Invalid key"))?;
    let ciphertxt = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| other_error("Encryption failed"))?;

    // - merge salt bytes (16 bytes) + nonce + ciphertxt
    let mut res = Vec::with_capacity(SL + NL + ciphertxt.len());
    res.extend_from_slice(&sb);
    res.extend_from_slice(&nonce);
    res.extend_from_slice(&ciphertxt);
 
    Ok(res)
}

/// decrypt with passphrase
/// Fmt : [salt(16bytes)] + [nonce(12bytes)] + [ciphertext]
pub fn decrypt(
    encrypted: &[u8],
    passphrase: &str
) -> Result<Vec<u8>, Error> {
    if encrypted.len() < SL + NL {
        return Err(invalid_data_error("Data too short"));
    }

    // - extract salt, nonce, ciphertext
    let salt_bytes = &encrypted[..SL];
    let salt = SaltString::encode_b64(salt_bytes)
        .map_err(|_| invalid_data_error("Invalid salt encoding"))?;

    let nonce = Nonce::from_slice(&encrypted[SL..SL + NL]);
    let ciphertext = &encrypted[SL + NL..];

    // - derive key from passphrase + salt
    let ag2 = Argon2::default();
    let pass_hash = ag2
        .hash_password(passphrase.as_bytes(), &salt)
        .map_err(|_| other_error("Argon2 hashing failed"))?;
    let hash = pass_hash.hash.unwrap();
    let key_bytes: [u8; 32] = hash.as_bytes()[..32]
        .try_into()
        .map_err(|_| other_error("Key derivation failed"))?;

    // - decrypt
    let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
        .map_err(|_| other_error("Invalid key"))?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| invalid_data_error("Wrong passphrase or corrupted data"))?;

    Ok(plaintext)
}
