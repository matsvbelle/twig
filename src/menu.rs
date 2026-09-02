//! Arrow-key menu for `twig list -i`: drawn on stderr in raw mode, erased again
//! once a choice is made, so stdout stays free for the path the caller prints.
use crate::out;
use std::io::{IsTerminal, Write};

/// One menu line; `item` is set on selectable rows.
pub struct Line {
    pub text: String,
    pub item: Option<usize>,
}

#[derive(Debug, PartialEq)]
pub enum Choice {
    Select(usize),
    /// New worktree named `.1`, branched from item `.0`.
    New(usize, String),
    /// Confirmed removal of the worktree at this item.
    Remove(usize),
    Cancel,
}

#[derive(Debug, PartialEq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Esc,
    Backspace,
    Delete,
    /// Ctrl-C, Ctrl-D or end of input.
    Cancel,
    Char(char),
    Other,
}

enum Mode {
    Browse,
    Naming(String),
    Confirm,
}

/// Bottom-line texts for the highlighted item; `remove` is None when it can't be removed.
pub struct Prompts<'a> {
    pub new_name: &'a dyn Fn(usize) -> String,
    pub remove: &'a dyn Fn(usize) -> Option<String>,
}

pub struct Menu<'a> {
    lines: &'a [Line],
    /// Line index of each selectable item, in order.
    items: Vec<usize>,
    cursor: usize,
    mode: Mode,
    prompts: Prompts<'a>,
    /// First visible line; terminal height (None: show everything).
    top: usize,
    rows: Option<usize>,
}

impl<'a> Menu<'a> {
    pub fn new(lines: &'a [Line], initial: usize, prompts: Prompts<'a>, rows: Option<usize>) -> Menu<'a> {
        let items: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].item.is_some()).collect();
        let cursor = items.iter().position(|&l| lines[l].item == Some(initial)).unwrap_or(0);
        Menu { lines, items, cursor, mode: Mode::Browse, prompts, top: 0, rows }
    }

    fn item(&self) -> usize {
        self.lines[self.items[self.cursor]].item.unwrap_or(0)
    }

    pub fn step(&mut self, key: Key) -> Option<Choice> {
        if key == Key::Cancel {
            return Some(Choice::Cancel);
        }
        let item = self.item();
        match &mut self.mode {
            Mode::Browse => match key {
                Key::Up | Key::Char('k') => self.cursor = self.cursor.saturating_sub(1),
                Key::Down | Key::Char('j') => self.cursor = (self.cursor + 1).min(self.items.len() - 1),
                Key::Enter | Key::Char(' ') => return Some(Choice::Select(item)),
                Key::Esc | Key::Char('q') => return Some(Choice::Cancel),
                Key::Char('n') => self.mode = Mode::Naming(String::new()),
                Key::Char('r' | 'd') | Key::Delete if (self.prompts.remove)(item).is_some() => self.mode = Mode::Confirm,
                _ => {}
            },
            Mode::Confirm => match key {
                Key::Char('y' | 'Y') => return Some(Choice::Remove(item)),
                _ => self.mode = Mode::Browse,
            },
            Mode::Naming(name) => match key {
                Key::Esc => self.mode = Mode::Browse,
                Key::Backspace => {
                    name.pop();
                }
                Key::Enter if !name.is_empty() => return Some(Choice::New(item, name.clone())),
                Key::Char(c) => name.push(c),
                _ => {}
            },
        }
        None
    }

    /// Visible lines plus the hint/prompt line; the highlighted row is marked and inverted.
    pub fn frame(&mut self) -> Vec<String> {
        let cur = self.items[self.cursor];
        let height = self.rows.map_or(self.lines.len(), |r| r.saturating_sub(1).clamp(1, self.lines.len()));
        if cur < self.top {
            self.top = cur;
        }
        if cur >= self.top + height {
            self.top = cur + 1 - height;
        }
        let mut frame: Vec<String> = self.lines[self.top..self.top + height]
            .iter()
            .enumerate()
            .map(|(i, l)| if self.top + i == cur { format!("> {}", highlight(&l.text)) } else { format!("  {}", l.text) })
            .collect();
        frame.push(match &self.mode {
            Mode::Browse => out::dim("↑/↓ move · Enter/Space switch · n new worktree · r/d/Del remove · q/Esc quit"),
            Mode::Naming(name) => format!("{}{name}", (self.prompts.new_name)(self.item())),
            Mode::Confirm => (self.prompts.remove)(self.item()).unwrap_or_default(),
        });
        frame
    }
}

/// Reverse video, re-asserted after every reset the coloured text already contains.
fn highlight(s: &str) -> String {
    if out::color_enabled() {
        format!("\x1b[7m{}\x1b[0m", s.replace("\x1b[0m", "\x1b[0m\x1b[7m"))
    } else {
        s.to_string()
    }
}

/// Run the menu on the terminal; `initial` is the item highlighted first.
pub fn run(lines: &[Line], initial: usize, prompts: Prompts) -> Choice {
    let mut menu = Menu::new(lines, initial, prompts, term_rows());
    if menu.items.is_empty() {
        return Choice::Cancel;
    }
    let _raw = RawMode::enter();
    let mut keys = Keys { read: read_byte, pending: || pending(50), pushback: None };
    let mut screen = Screen::default();
    let choice = loop {
        let typing = !matches!(menu.mode, Mode::Browse);
        screen.draw(&menu.frame(), typing);
        if let Some(c) = menu.step(keys.next()) {
            break c;
        }
    };
    screen.clear();
    choice
}

/// Redraws the frame in place on stderr; the cursor stays on the last line so
/// the terminal scrolls naturally and the typed name shows where it is typed.
#[derive(Default)]
struct Screen {
    drawn: usize,
}

impl Screen {
    fn draw(&mut self, frame: &[String], cursor_visible: bool) {
        let mut s = self.rewind();
        for (i, line) in frame.iter().enumerate() {
            if i > 0 {
                s.push('\n');
            }
            s.push_str(line);
            s.push_str("\x1b[K");
        }
        s.push_str(if cursor_visible { "\x1b[?25h" } else { "\x1b[?25l" });
        write(&s);
        self.drawn = frame.len();
    }

    fn clear(&mut self) {
        write(&format!("{}\x1b[J\x1b[?25h", self.rewind()));
        self.drawn = 0;
    }

    fn rewind(&self) -> String {
        match self.drawn {
            0 => String::new(),
            1 => "\r".to_string(),
            n => format!("\r\x1b[{}A", n - 1),
        }
    }
}

fn write(s: &str) {
    let mut e = std::io::stderr().lock();
    let _ = e.write_all(s.as_bytes());
    let _ = e.flush();
}

/// Key decoder over a byte source; `pending` tells a lone Esc from an escape sequence.
struct Keys<R: FnMut() -> Option<u8>, P: FnMut() -> bool> {
    read: R,
    pending: P,
    pushback: Option<u8>,
}

impl<R: FnMut() -> Option<u8>, P: FnMut() -> bool> Keys<R, P> {
    fn byte(&mut self) -> Option<u8> {
        self.pushback.take().or_else(|| (self.read)())
    }

    fn next(&mut self) -> Key {
        let Some(b) = self.byte() else { return Key::Cancel };
        match b {
            0x1b => self.escape(),
            b'\r' | b'\n' => Key::Enter,
            0x7f | 0x08 => Key::Backspace,
            0x03 | 0x04 => Key::Cancel,
            0x00..=0x1f => Key::Other,
            0x20..=0x7e => Key::Char(b as char),
            _ => self.utf8(b),
        }
    }

    fn escape(&mut self) -> Key {
        if !(self.pending)() {
            return Key::Esc;
        }
        match self.byte() {
            Some(b'[') | Some(b'O') => {}
            other => {
                self.pushback = other;
                return Key::Esc;
            }
        }
        // CSI: parameter bytes, then a final byte in 0x40..=0x7e (`3~` is Delete).
        let mut params = Vec::new();
        loop {
            match self.byte() {
                Some(b'A') => return Key::Up,
                Some(b'B') => return Key::Down,
                Some(b'~') if params == b"3" => return Key::Delete,
                Some(0x40..=0x7e) => return Key::Other,
                Some(b) => params.push(b),
                None => return Key::Cancel,
            }
        }
    }

    fn utf8(&mut self, first: u8) -> Key {
        let extra = match first {
            0xc0..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf7 => 3,
            _ => return Key::Other,
        };
        let mut bytes = vec![first];
        for _ in 0..extra {
            match self.byte() {
                Some(b) => bytes.push(b),
                None => return Key::Cancel,
            }
        }
        std::str::from_utf8(&bytes).ok().and_then(|s| s.chars().next()).map_or(Key::Other, Key::Char)
    }
}

fn read_byte() -> Option<u8> {
    let mut b = 0u8;
    loop {
        let n = unsafe { libc::read(0, &mut b as *mut u8 as *mut libc::c_void, 1) };
        if n == 1 {
            return Some(b);
        }
        if n == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return None;
        }
    }
}

/// Whether stdin has a byte within `ms` milliseconds.
fn pending(ms: i32) -> bool {
    let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    unsafe { libc::poll(&mut p, 1, ms) > 0 }
}

fn term_rows() -> Option<usize> {
    if !std::io::stderr().is_terminal() {
        return None;
    }
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(2, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_row > 0;
    ok.then_some(ws.ws_row as usize)
}

/// Raw stdin for its lifetime; a no-op when stdin isn't a terminal (bytes are then read as-is).
struct RawMode(Option<libc::termios>);

impl RawMode {
    fn enter() -> RawMode {
        if !std::io::stdin().is_terminal() {
            return RawMode(None);
        }
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(0, &mut t) } != 0 {
            return RawMode(None);
        }
        let orig = t;
        // ISIG off: Ctrl-C arrives as a byte so the terminal is restored by us, not left raw.
        t.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG | libc::IEXTEN);
        t.c_iflag &= !(libc::IXON | libc::ICRNL);
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) };
        RawMode(Some(orig))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if let Some(t) = &self.0 {
            unsafe { libc::tcsetattr(0, libc::TCSANOW, t) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<Line> {
        let mk = |t: &str, item| Line { text: t.into(), item };
        vec![mk("header", None), mk("b1/", None), mk("alpha", Some(0)), mk("b2/", None), mk("alpha", Some(1)), mk("beta", Some(2))]
    }

    fn new_name(i: usize) -> String {
        format!("New from {i}: ")
    }

    /// Item 0 is not removable (a main repo, say).
    fn remove(i: usize) -> Option<String> {
        (i > 0).then(|| format!("Remove {i}? [y/N] "))
    }

    fn prompts() -> Prompts<'static> {
        Prompts { new_name: &new_name, remove: &remove }
    }

    #[test]
    fn navigation_skips_headers_and_stops_at_ends() {
        let lines = lines();
        let mut m = Menu::new(&lines, 1, prompts(), None);
        assert_eq!(m.item(), 1);
        assert_eq!(m.step(Key::Up), None);
        assert_eq!(m.item(), 0);
        assert_eq!(m.step(Key::Up), None);
        assert_eq!(m.item(), 0);
        for _ in 0..5 {
            assert_eq!(m.step(Key::Char('j')), None);
        }
        assert_eq!(m.item(), 2);
        assert_eq!(m.step(Key::Char('k')), None);
        assert_eq!(m.step(Key::Char(' ')), Some(Choice::Select(1)));
        assert_eq!(m.step(Key::Enter), Some(Choice::Select(1)));
        assert_eq!(m.step(Key::Char('q')), Some(Choice::Cancel));
        assert_eq!(m.step(Key::Esc), Some(Choice::Cancel));
        assert_eq!(m.step(Key::Cancel), Some(Choice::Cancel));
        assert_eq!(Menu::new(&lines, 9, prompts(), None).item(), 0, "unknown initial falls back to the first item");
    }

    #[test]
    fn naming_edits_and_escapes() {
        let lines = lines();
        let mut m = Menu::new(&lines, 2, prompts(), None);
        assert_eq!(m.step(Key::Char('n')), None);
        for k in [Key::Char('a'), Key::Char('b'), Key::Backspace, Key::Char('c'), Key::Down, Key::Other] {
            assert_eq!(m.step(k), None);
        }
        assert_eq!(m.frame().last().unwrap(), "New from 2: ac");
        assert_eq!(m.step(Key::Enter), Some(Choice::New(2, "ac".into())));
        assert_eq!(m.step(Key::Esc), None, "Esc leaves naming mode");
        assert_eq!(m.step(Key::Char('n')), None);
        assert_eq!(m.step(Key::Enter), None, "empty name is not accepted");
        assert_eq!(m.step(Key::Cancel), Some(Choice::Cancel));
    }

    #[test]
    fn remove_asks_first_and_skips_unremovable_items() {
        let lines = lines();
        let mut m = Menu::new(&lines, 1, prompts(), None);
        for k in [Key::Char('r'), Key::Char('d'), Key::Delete] {
            assert_eq!(m.step(k), None);
            assert_eq!(m.frame().last().unwrap(), "Remove 1? [y/N] ");
            assert_eq!(m.step(Key::Char('n')), None, "anything but y backs out");
            assert!(m.frame().last().unwrap().contains("r/d/Del remove"));
        }
        m.step(Key::Char('d'));
        assert_eq!(m.step(Key::Esc), None);
        m.step(Key::Char('d'));
        assert_eq!(m.step(Key::Char('y')), Some(Choice::Remove(1)));
        m.step(Key::Char('d'));
        assert_eq!(m.step(Key::Cancel), Some(Choice::Cancel));
        m.step(Key::Up);
        assert_eq!(m.step(Key::Delete), None);
        assert!(m.frame().last().unwrap().contains("r/d/Del remove"), "item 0 can't be removed: still browsing");
        assert_eq!(m.step(Key::Char('y')), None, "y in browse mode does nothing");
    }

    #[test]
    fn frame_marks_cursor_and_scrolls() {
        let lines = lines();
        let mut m = Menu::new(&lines, 0, prompts(), None);
        let f = m.frame();
        assert_eq!(f.len(), 7);
        assert_eq!(f[2], "> alpha");
        assert_eq!(f[1], "  b1/");
        assert!(f[6].contains("n new worktree"));

        let mut m = Menu::new(&lines, 0, prompts(), Some(4));
        assert_eq!(m.frame()[..3], ["  header", "  b1/", "> alpha"]);
        m.step(Key::Down);
        m.step(Key::Down);
        assert_eq!(m.frame()[..3], ["  b2/", "  alpha", "> beta"]);
        m.step(Key::Up);
        m.step(Key::Up);
        assert_eq!(m.frame()[..3], ["> alpha", "  b2/", "  alpha"]);
    }

    fn decode(bytes: &[u8], pending: bool) -> Vec<Key> {
        let mut it = bytes.iter().copied();
        let mut keys = Keys { read: || it.next(), pending: || pending, pushback: None };
        let mut out = Vec::new();
        loop {
            let k = keys.next();
            let done = k == Key::Cancel;
            out.push(k);
            if done {
                return out;
            }
        }
    }

    #[test]
    fn key_decoding() {
        use Key::*;
        assert_eq!(decode(b"\x1b[B\x1b[A\x1bOAq \r\n\x7f\x08\x1b[3~\x1b[1;5C\x1b[2~\x01\xc3\xa9\x03", true), vec![Down, Up, Up, Char('q'), Char(' '), Enter, Enter, Backspace, Backspace, Delete, Other, Other, Other, Char('é'), Cancel]);
        assert_eq!(decode(b"\x1bq", true), vec![Esc, Char('q'), Cancel], "Esc followed by a key keeps the key");
        assert_eq!(decode(b"\x1b", false), vec![Esc, Cancel]);
        assert_eq!(decode(b"\x04", true), vec![Cancel]);
        assert_eq!(decode(b"", true), vec![Cancel]);
    }
}
