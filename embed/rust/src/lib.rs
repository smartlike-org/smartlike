extern crate hex;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate blake2;
extern crate ed25519_dalek;
use serde::{Serialize, Deserialize};

pub mod client;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Response<T> {
    pub status: String,
    pub data: T,
}
