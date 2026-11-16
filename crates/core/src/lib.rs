//! Questodian's core: rules, dice, the SRD reference, records and the HTTP API.
//! Shared by the desktop app and the command line; knows nothing about windows.

pub mod db;
pub mod dice;
pub mod error;
pub mod game;
pub mod kinds;
pub mod repo;
pub mod rules;
pub mod schemas;
pub mod server;
pub mod srd;
