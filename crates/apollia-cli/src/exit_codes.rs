//! POSIX exit codes for the Apollia CLI.

/// Success — command completed without errors.
pub const SUCCESS: i32 = 0;

/// General error — usage error, invalid input, etc.
pub const GENERAL_ERROR: i32 = 1;

/// Runtime error — runtime not started, connection refused.
pub const RUNTIME_ERROR: i32 = 2;

/// Task failed — `run` command with a failed task.
pub const TASK_FAILED: i32 = 3;

/// Timeout — operation exceeded its deadline.
pub const TIMEOUT: i32 = 4;

/// Interrupted — runtime was stopped by Ctrl+C (SIGINT).
pub const INTERRUPTED: i32 = 5;
