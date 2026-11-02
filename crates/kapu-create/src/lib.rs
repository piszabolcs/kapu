//! KAPU-készítő: web sites, documents and folders to a KAPU file
//! (kapu-notes/TERV-KESZITO.md, KD1-KD16).
#![forbid(unsafe_code)]

pub mod create;
pub mod detect;
pub mod doc;
pub mod error;
pub mod markdown;
pub mod readers;
pub mod rooms;
pub mod text;
pub mod web;

pub use doc::{Block, Document, Kind};
pub use error::{Error, Result};
