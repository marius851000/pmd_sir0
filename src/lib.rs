#![warn(missing_docs)]
//! This crate allow you to read Sir0 file, used on pokemon mystery dungeon on nintendo 3DS.
//!
//! The Sir0 file contain a list of pointer to various part in the file.

mod sir0;
pub use sir0::{Sir0, Sir0Error};
pub use sir0::{write_sir0_footer, write_sir0_header};
