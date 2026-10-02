//! The Rust side of the interface.
//!
//! `app.slint` declares what can be pressed and what is shown; this is where
//! each press becomes a command (`actions`) and each piece of state reaches a
//! property (`sync`). Beside them sit the idle clock the chrome hides on and
//! the one subtitle line both the person and the bar get a say in placing.

pub mod actions;
pub mod chrome;
pub mod format;
pub mod subline;
pub mod sync;
