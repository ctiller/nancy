use did_key::{Ed25519KeyPair, generate};
use did_key::KeyMaterial;

fn main() {
    let key = generate::<Ed25519KeyPair>(None);
    let sec = key.private_key_bytes();
    let key2 = Ed25519KeyPair::from_secret_key(&sec);
}
