# Assumes a C toolchain is installed, eg. build-essentials and OpenSSL

# Install just
apt install just

# Install rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
rustup target add wasm32-unknown-unknown

# Rust specific dependencies
rustup default nightly
cargo install trunk wasm-bindgen-cli