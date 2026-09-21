//! End-to-end tests against the built binary; one module per command under `cli/`.
mod util;

mod cli {
    mod add;
    mod init;
    mod list;
    mod open;
    mod prune;
    mod remove;
    mod repos;
    mod shell;
}
