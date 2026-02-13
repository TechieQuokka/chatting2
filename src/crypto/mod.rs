mod aead;
mod enc_file;
mod kdf;
#[cfg(test)]
mod tests;

pub use aead::{decrypt, encrypt, EncryptedData};
pub use enc_file::{load_enc, save_enc, EncFileError};
pub use kdf::derive_key;
