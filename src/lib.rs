//! Wez Expect Library
//!
//! This library provides core functionality for Wez Expect - a tool for monitoring
//! WezTerm terminal output and responding with automated actions.
//!
//! The library supports 4 built-in action types:
//! - `wait_for_time`: Extract time/timezone from terminal output and wait until that time
//! - `immediate`: Send a command immediately when a pattern matches
//! - `conditional`: Test captured groups and send commands based on conditions
//! - `external_command`: Execute shell commands with capture group substitution

// Declare all modules (they're in separate files)
pub mod action;
mod config;
mod monitor;
mod pane;
mod parser;
mod scheduler;
pub mod sender; // Public for testing command sending
mod utils;

// Re-export internal modules for the binary
pub use config::Config;
pub use monitor::Monitor;
pub use pane::select_pane;
