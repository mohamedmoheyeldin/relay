use thiserror::Error;

/// Which time limit fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutKind {
    Total,
    Idle,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AiError {
    #[error("the AI command-line tool was not found ({program}); install it and make sure it is on PATH")]
    NotInstalled { program: String },
    #[error("not signed in: {0}")]
    NotLoggedIn(String),
    #[error("the run timed out ({0:?})")]
    Timeout(TimeoutKind),
    #[error("the run was cancelled")]
    Cancelled,
    #[error("could not start the AI process: {0}")]
    Spawn(String),
    #[error("{message}")]
    Cli {
        message: String,
        exit_code: Option<i32>,
    },
    #[error("i/o error talking to the AI process: {0}")]
    Io(String),
}
