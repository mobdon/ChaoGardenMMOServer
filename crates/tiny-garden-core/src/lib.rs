//! Native binary inspection and a separate, custom MMO training adaptation.
//!
//! This crate does not execute a ROM, emulate GBA hardware, decode unknown
//! Chao save fields, or connect phone clients to the production game server.
#![forbid(unsafe_code)]

pub mod gba;
pub mod native_growth;
pub mod native_hatching;
pub mod native_memory;
pub mod native_rps;
pub mod native_save;
pub mod native_shop_state;
pub mod native_weeds;
pub mod shop;
pub mod training;
