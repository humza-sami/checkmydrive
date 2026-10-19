//! CheckMyDrive engine: disk health / fake-capacity testing. See `docs/CONTRACT.md`.

pub mod blockdev;
pub mod cleanup;
pub mod cli;
pub mod engine;
pub mod platform;
pub mod progress;
pub mod report;
pub mod score;
pub mod smart;
pub mod tests;
pub mod util;
