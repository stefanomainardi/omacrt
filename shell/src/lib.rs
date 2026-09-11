//! Shared pieces of omacrt: the game library, the video player and
//! fitting logic, settings, the TV profile and the CRT output control used
//! by both the launcher (`omacrt-shell`) and the CLI (`omacrt`).

pub mod ambient;
pub mod assets;
pub mod colour;
pub mod config;
pub mod coredata;
pub mod covers;
pub mod crt;
pub mod game;
pub mod immich;
pub mod index;
pub mod library;
pub mod logfile;
pub mod music;
pub mod net;
pub mod padmap;
pub mod player;
pub mod plugin;
pub mod profile;
pub mod rumble;
pub mod scumm;
pub mod settings;
pub mod states;
pub mod store;
pub mod term;
pub mod theme;
pub mod videofit;
pub mod yt;
