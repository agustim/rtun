# RTun

Generate a VPN in rust 

disclaimer: this is my first rust program, feel free to send pull request.  

## Devel

### 1st Step - Start project

``` 
$ cargo init
     Created binary (application) package
```

### 2nd Step - Add libs
```
$ cargo add clap --features derive
```

### 3rd Step - UDP Server and client
Using tokio:select! and spawn elements

### 4rt Step - Mutex between threads
Using Mutex and space for MutexGuard


### 5rt Step - Reduce binary

```
cargo install --force cargo-strip
cargo strip
```
When cross-compiling, use ```--target```


### 6rt Step - Crypto

First version, encrypt and decrypt with static key (in code OMG!) with ChaCha20


### 7rt Step - Static build

```
RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target x86_64-unknown-linux-gnu
```

### 8rt Step - Add action workflow

https://github.com/marketplace/actions/build-and-upload-rust-binary-to-github-releases
