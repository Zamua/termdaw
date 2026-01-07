//! TermDAW - A terminal-based Digital Audio Workstation
//!
//! This library exposes the core functionality of TermDAW for testing
//! and potential embedding use cases.

#![deny(warnings)]

pub mod app;
pub mod arrangement;
pub mod audio;
pub mod audio_sync;
pub mod browser;
pub mod command_picker;
pub mod coords;
pub mod cursor;
pub mod effects;
pub mod history;
pub mod input;
pub mod mixer;
pub mod mode;
pub mod playback;
pub mod plugin_host;
pub mod project;
pub mod sequencer;
pub mod ui;
