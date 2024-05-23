


    use log::debug;
    use crypto::symmetriccipher::SynchronousStreamCipher;

    pub fn encrypt (key: &[u8], iv: &[u8], msg: &[u8]) -> Vec<u8> {
        let mut c = crypto::chacha20::ChaCha20::new(&key, iv);
        let mut output: Vec<u8> = std::iter::repeat(0).take(msg.len()).collect();
        debug!("Encrypting message");
        c.process(&msg[..], &mut output[..]);
        return output;
    }
    
    
    pub fn decrypt (key: &[u8], iv: &[u8], msg: Vec<u8>, ret: &mut [u8]) {
        let mut c = crypto::chacha20::ChaCha20::new(&key, iv);
        let mut output = msg;
        let mut newoutput: Vec<u8> = std::iter::repeat(0).take(output.len()).collect();
        debug!("Decrypting message");
        c.process(&mut output[..], &mut newoutput[..]);
        ret.copy_from_slice(&newoutput[..]);
    }
