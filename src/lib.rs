#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "tstd")]
#[macro_use]
extern crate sgxlib as std;

mod buffer;
pub use buffer::*;
mod ring_vec;
pub use ring_vec::*;