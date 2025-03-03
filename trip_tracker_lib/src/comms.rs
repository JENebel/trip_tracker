pub const SIGNATURE_SIZE: usize = 16; // bytes

pub trait MacProvider {
    /// Should perform a HMAC-SHA256 signature of the data using the token as the key.
    /// So SHA256(data || tooken). This should be safe (!?) and prevent extension attacks according to Wikipedia
    fn sign(data: &[u8], token: &[u8]) -> [u8; SIGNATURE_SIZE];

    fn verify(data: &[u8], signature: &[u8], key: &[u8]) -> bool {
        Self::sign(data, key) == *signature
    }
}