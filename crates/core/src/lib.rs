//! Questodian's core: rules, dice, the SRD reference, records and the HTTP API.
//! Shared by the desktop app, the command line and the web build; knows nothing about windows.

pub mod builder;
pub mod db;
pub mod dice;
pub mod error;
pub mod files;
pub mod game;
pub mod kinds;
pub mod repo;
pub mod rules;
pub mod schemas;
pub mod server;
pub mod srd;
pub mod sync;
pub mod transfer;
