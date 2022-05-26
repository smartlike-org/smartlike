//! Smartlike RPC definitions and types.
//!
//! ## Client library for connecting to Smartlike network
//!
//!

extern crate hex;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate blake2;
extern crate ed25519_dalek;

pub mod client;
