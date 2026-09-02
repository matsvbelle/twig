//! Progress output. Commands that land you somewhere run in terminal mode:
//! everything goes to stderr so the shell wrapper captures only the final path
//! printed with `result()` (nothing at all with `-o`, which opens the IDE).
use crate::cli::Color;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static TERMINAL: AtomicBool = AtomicBool::new(false);
static COLOR: AtomicU8 = AtomicU8::new(0);

pub fn set_color(mode: Color) {
    COLOR.store(mode as u8, Ordering::Relaxed);
}

/// `auto` looks at the stream `say` writes to, so wrapper-captured stdout stays plain.
pub fn color_enabled() -> bool {
    match COLOR.load(Ordering::Relaxed) {
        x if x == Color::Always as u8 => true,
        x if x == Color::Auto as u8 => {
            if terminal_mode() { std::io::stderr().is_terminal() } else { std::io::stdout().is_terminal() }
        }
        _ => false,
    }
}

fn paint(code: &str, s: &str) -> String {
    if color_enabled() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}

pub fn set_terminal_mode() {
    TERMINAL.store(true, Ordering::Relaxed);
}


pub fn terminal_mode() -> bool {
    TERMINAL.load(Ordering::Relaxed)
}

pub fn say(msg: impl AsRef<str>) {
    if terminal_mode() {
        eprintln!("{}", msg.as_ref());
    } else {
        println!("{}", msg.as_ref());
    }
}

pub fn warn(msg: impl AsRef<str>) {
    eprintln!("{}", msg.as_ref());
}

/// The one line meant for the shell wrapper (a path to cd into).
pub fn result(path: &std::path::Path) {
    println!("{}", path.display());
}

pub fn green(s: &str) -> String {
    paint("92", s)
}

pub fn yellow(s: &str) -> String {
    paint("93", s)
}

pub fn red(s: &str) -> String {
    paint("91", s)
}

pub fn bold(s: &str) -> String {
    paint("1", s)
}

pub fn dim(s: &str) -> String {
    paint("2", s)
}

pub fn cyan(s: &str) -> String {
    paint("96", s)
}
