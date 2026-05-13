// Shared helpers for integration tests. Cargo treats `tests/common/mod.rs`
// as a *module*, not its own integration target, so this file is brought in
// via `mod common;` in each test file rather than producing its own binary.

#![allow(dead_code)] // not all helpers are used by every test target

pub mod server;
