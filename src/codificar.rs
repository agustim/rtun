
use crypto::symmetriccipher::SynchronousStreamCipher;
use log::{debug,error};
use rustc_serialize::hex::FromHex;
use std::iter::repeat;


pub const MAX_KEY_SIZE: usize = 32;
pub const MAX_IV_SIZE: usize = 12;

// Encrypt and Decrypt functions
pub fn encrypt(key: &[u8], iv: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut c = crypto::chacha20::ChaCha20::new(&key, iv);
    let mut output: Vec<u8> = repeat(0).take(msg.len()).collect();
    debug!("Encrypting message");
    c.process(&msg[..], &mut output[..]);
    return output;
}

pub fn decrypt(key: &[u8], iv: &[u8], msg: Vec<u8>, ret: &mut [u8]) {
    let mut c = crypto::chacha20::ChaCha20::new(&key, iv);
    let mut output = msg;
    let mut newoutput: Vec<u8> = repeat(0).take(output.len()).collect();
    debug!("Decrypting message");
    c.process(&mut output[..], &mut newoutput[..]);
    ret.copy_from_slice(&newoutput[..]);
}

//Segur que hi ha una forma més correcte!!
//Convert &[u8] to [u8;MAX_KEY_SIZE]
pub fn key_to_array(key: &[u8]) -> Result<[u8; MAX_KEY_SIZE], &'static str> {
    let mut key_array = [0u8; MAX_KEY_SIZE];
    let key_len = key.len();

    if key_len > MAX_KEY_SIZE {
        return Err("Key size is too big");
    }
    for i in 0..key_len {
        key_array[i] = key[i];
    }
    // Fill the rest with zero
    for i in key_len..MAX_KEY_SIZE {
        key_array[i] = 0;
    }
    return Ok(key_array);
}
//Convert &[u8] to [u8;MAX_IV_SIZE]
pub fn iv_to_array(k: &[u8]) -> Result<[u8; MAX_IV_SIZE], &'static str> {
    let mut ret_array = [0u8; MAX_IV_SIZE];
    let k_len = k.len();

    if k_len > MAX_IV_SIZE {
        return Err("IV size is too big");
    }
    for i in 0..k_len {
        ret_array[i] = k[i];
    }
    // Fill the rest with zero
    for i in k_len..MAX_IV_SIZE {
        ret_array[i] = 0;
    }
    return Ok(ret_array);
}

pub fn hex_to_bytes(s: &str) -> Vec<u8> {
    match s.from_hex() {
        Ok(r) => return r,
        Err(e) => {
            error!("Error: {:?}, this string contains some different character to [0-9a-f]. Generate key with all zero.", e);
            //return vector with 0
            return repeat(0).take(0).collect();
        },
    };
}