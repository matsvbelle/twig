//! `twig list`: the generated worktrees as a tree (`-r`: below the main repos;
//! `-i`: as a menu to switch from, branch a new worktree off, or remove one).
use crate::cli::ListArgs;
use crate::error::Result;
use crate::menu::{self, Choice, Line, Prompts};
use crate::remove::Remover;
use crate::root::Root;
use crate::status::{Job, Status};
use crate::{add, bail, git, out};
use std::path::{Path, PathBuf};

/// A listed checkout: a generated worktree, or (with `-r`) a main repo.
pub struct Entry {
    pub row: Row,
    pub path: PathBuf,
    pub main: PathBuf,
    /// Checked-out branch (None: detached).
    pub branch: Option<String>,
    pub is_main: bool,
}

impl Entry {
    fn label(&self) -> String {
        if self.is_main { self.row.repo.clone() } else { format!("{}/{}", self.row.branch_dir, self.row.repo) }
    }

    /// Base ref for a worktree branched off this checkout.
    fn base(&self) -> Result<String> {
        match (&self.branch, self.is_main) {
            (Some(b), _) => Ok(b.clone()),
            (None, true) => Ok("HEAD".into()),
            (None, false) => bail!("Cannot branch from {}: detached HEAD.", self.label()),
        }
    }
}

pub fn run(args: ListArgs, open: bool) -> Result<()> {
    if args.interactive_switch {
        out::set_terminal_mode();
    }
    let root = Root::discover()?;
    let filter = if args.all { None } else { Some(current_repo(&root)?) };
    let scope = match &filter {
        None => "(all repos)".to_string(),
        Some(r) => format!("(repo: {r})"),
    };
    // After a removal the menu comes back, highlighting the row that took the removed one's place.
    let mut resume: Option<usize> = None;
    loop {
        let entries = collect(&root, &args, filter.as_deref());
        if entries.is_empty() {
            if root.worktrees().is_empty() {
                out::say(format!("No worktrees under {}", root.worktrees_dir().display()));
            } else {
                out::say(format!("{}  {scope}", root.config.worktrees));
                out::say(format!("  (none for {})", filter.unwrap_or_default()));
            }
            return Ok(());
        }
        let lines = render(&root, &args, &scope, &entries);
        if !args.interactive_switch {
            for l in &lines {
                println!("{}", l.text);
            }
            return Ok(());
        }

        let initial = resume.unwrap_or_else(|| entries.iter().position(|e| e.row.current).unwrap_or(0)).min(entries.len() - 1);
        let new_name = |i: usize| format!("New worktree from {} [{}]: ", entries[i].label(), entries[i].base().unwrap_or_else(|_| "detached".into()));
        let remove = |i: usize| {
            let e = &entries[i];
            if e.is_main {
                return None;
            }
            let marks = match (&e.branch, args.long) {
                (_, true) => e.row.marks.clone(),
                (Some(b), false) => Status::probe(&e.path, b, None).marks(),
                (None, false) => String::new(),
            };
            let here = if e.row.current { " (you'll land in the main repo)" } else { "" };
            Some(format!("Remove worktree {}{marks}{here}? [y/N] ", e.label()))
        };
        match menu::run(&lines, initial, Prompts { new_name: &new_name, remove: &remove }) {
            Choice::Cancel => return Ok(()),
            Choice::Select(i) => {
                out::say(format!("→ {}", entries[i].label()));
                add::land(&root, &entries[i].path, open);
                return Ok(());
            }
            Choice::New(i, name) => {
                let e = &entries[i];
                let base = e.base()?;
                return add::create(&root, &e.main, &name, Some(&base), open);
            }
            Choice::Remove(i) => {
                let e = &entries[i];
                Remover::new(root.clone(), open).remove_all(std::slice::from_ref(&e.path))?;
                if e.row.current {
                    return Ok(());
                }
                resume = Some(i);
            }
        }
    }
}

/// Main repos (with `-r`) followed by worktrees, restricted to `filter` when given.
fn collect(root: &Root, args: &ListArgs, filter: Option<&str>) -> Vec<Entry> {
    let current = std::env::current_dir().ok().and_then(|d| git::toplevel(&d));
    let mut entries: Vec<Entry> = Vec::new();
    if args.root_repos {
        let repos = match filter {
            Some(r) => vec![root.dir.join(r)],
            None => root.repos(),
        };
        for main in repos {
            let branch = git::current_branch(&main);
            let row = Row {
                branch_dir: String::new(),
                repo: root.repo_name(&main),
                branch: Some(branch.clone().unwrap_or_default()),
                current: current.as_deref() == Some(main.as_path()),
                marks: String::new(),
            };
            entries.push(Entry { row, path: main.clone(), main, branch, is_main: true });
        }
    }
    for w in root.worktrees().iter().filter(|w| filter.is_none_or(|r| w.repo == r)) {
        let branch = w.branch();
        let row = Row {
            branch_dir: w.branch_dir.clone(),
            repo: w.repo.clone(),
            branch: Some(branch.clone().unwrap_or_default()),
            current: current.as_deref() == Some(w.path.as_path()),
            marks: String::new(),
        };
        entries.push(Entry { row, path: w.path.clone(), main: root.dir.join(&w.repo), branch, is_main: false });
    }
    if args.long {
        let with_branch: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].branch.is_some()).collect();
        let jobs: Vec<Job> = with_branch.iter().map(|&i| (entries[i].path.as_path(), entries[i].branch.as_deref().unwrap_or_default(), None)).collect();
        let statuses = Status::probe_all(&jobs);
        for (&i, status) in with_branch.iter().zip(statuses) {
            entries[i].row.marks = status.marks();
        }
    }
    entries
}

fn render(root: &Root, args: &ListArgs, scope: &str, entries: &[Entry]) -> Vec<Line> {
    let rows: Vec<&Row> = entries.iter().map(|e| &e.row).collect();
    if !args.root_repos {
        return render_lines(&format!("{}  {scope}", root.config.worktrees), &group(&rows, 0));
    }
    let mains = entries.iter().filter(|e| e.is_main).count();
    let mut nodes: Vec<Node> = rows[..mains].iter().enumerate().map(|(i, r)| Node::Leaf(i, r)).collect();
    nodes.push(Node::Dir(root.config.worktrees.clone(), group(&rows[mains..], mains)));
    render_lines(&format!("{}  {scope}", tilde(&root.dir)), &nodes)
}

/// Name of the main repo for the checkout at the cwd.
pub fn current_repo(root: &Root) -> Result<String> {
    match root.main_repo_at(&std::env::current_dir()?) {
        Ok(main) => Ok(root.repo_name(&main)),
        Err(_) => Err("Not inside a git repo. Use -A to list all repos.".into()),
    }
}

/// `$HOME` shown as `~` (display only).
fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub struct Row {
    pub branch_dir: String,
    pub repo: String,
    /// Shown as `[branch]` when Some (empty: detached).
    pub branch: Option<String>,
    pub current: bool,
    /// Pre-rendered status marks (`Status::marks`), may be empty.
    pub marks: String,
}

pub enum Node<'a> {
    Dir(String, Vec<Node<'a>>),
    /// Selectable row with its item index.
    Leaf(usize, &'a Row),
}

/// Rows grouped by branch dir in input order; item indices start at `offset`.
pub fn group<'a>(rows: &[&'a Row], offset: usize) -> Vec<Node<'a>> {
    let mut nodes: Vec<Node> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let leaf = Node::Leaf(offset + i, row);
        match nodes.last_mut() {
            Some(Node::Dir(name, items)) if *name == row.branch_dir => items.push(leaf),
            _ => nodes.push(Node::Dir(row.branch_dir.clone(), vec![leaf])),
        }
    }
    nodes
}

pub fn render_lines(header: &str, nodes: &[Node]) -> Vec<Line> {
    let mut lines = vec![Line { text: out::bold(header), item: None }];
    render_nodes(nodes, "", &mut lines);
    lines
}

fn has_current(node: &Node) -> bool {
    match node {
        Node::Leaf(_, row) => row.current,
        Node::Dir(_, children) => children.iter().any(has_current),
    }
}

fn render_nodes(nodes: &[Node], indent: &str, out: &mut Vec<Line>) {
    for (i, node) in nodes.iter().enumerate() {
        let (pre, cont) = if i + 1 == nodes.len() { ("└── ", "    ") } else { ("├── ", "│   ") };
        let connector = out::dim(&format!("{indent}{pre}"));
        match node {
            Node::Dir(name, children) => {
                let name = format!("{name}/");
                let text = format!("{connector}{}", if has_current(node) { out::cyan(&name) } else { name });
                out.push(Line { text, item: None });
                render_nodes(children, &format!("{indent}{cont}"), out);
            }
            Node::Leaf(idx, row) => {
                // The branch column lines up across depths (22 wide at the usual depth of two).
                let width = 30usize.saturating_sub(indent.chars().count() + pre.chars().count());
                let repo = if row.branch.is_some() || row.current { format!("{:<width$}", row.repo) } else { row.repo.clone() };
                let mut line = if row.current { out::cyan(&repo) } else { repo };
                if let Some(b) = &row.branch {
                    line.push_str(&format!(" {}", out::dim(&format!("[{b}]"))));
                }
                if row.current {
                    line.push_str(&format!(" {}", out::green("← here")));
                }
                out.push(Line { text: format!("{connector}{line}{}", row.marks), item: Some(*idx) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(branch_dir: &str, repo: &str, branch: Option<&str>, current: bool, marks: &str) -> Row {
        Row { branch_dir: branch_dir.into(), repo: repo.into(), branch: branch.map(str::to_string), current, marks: marks.into() }
    }

    fn text(lines: &[Line]) -> String {
        lines.iter().map(|l| format!("{}\n", l.text)).collect()
    }

    #[test]
    fn tree_connectors() {
        let rows = [row("b1", "alpha", Some("b1"), false, ""), row("b1", "beta", Some("b1"), true, " [dirty]"), row("b2", "alpha", Some("b2"), false, "")];
        let refs: Vec<&Row> = rows.iter().collect();
        let lines = render_lines(".WORKTREES  (all repos)", &group(&refs, 0));
        let expected = "\
.WORKTREES  (all repos)
├── b1/
│   ├── alpha                  [b1]
│   └── beta                   [b1] ← here [dirty]
└── b2/
    └── alpha                  [b2]
";
        assert_eq!(text(&lines), expected);
        let items: Vec<Option<usize>> = lines.iter().map(|l| l.item).collect();
        assert_eq!(items, vec![None, None, Some(0), Some(1), None, Some(2)]);
    }

    #[test]
    fn root_repos_nest_the_worktrees_folder() {
        let roots = [row("", "alpha", Some("main"), true, ""), row("", "beta", None, false, "")];
        let wts = [row("b1", "alpha", Some("b1"), false, ""), row("b1", "beta", Some("b1"), false, ""), row("b2", "alpha", Some("b2"), false, "")];
        let refs: Vec<&Row> = wts.iter().collect();
        let mut nodes: Vec<Node> = roots.iter().enumerate().map(|(i, r)| Node::Leaf(i, r)).collect();
        nodes.push(Node::Dir(".WORKTREES".into(), group(&refs, roots.len())));
        let lines = render_lines("~/Projects  (all repos)", &nodes);
        let expected = "\
~/Projects  (all repos)
├── alpha                      [main] ← here
├── beta
└── .WORKTREES/
    ├── b1/
    │   ├── alpha              [b1]
    │   └── beta               [b1]
    └── b2/
        └── alpha              [b2]
";
        assert_eq!(text(&lines), expected);
        let items: Vec<Option<usize>> = lines.iter().map(|l| l.item).collect();
        assert_eq!(items, vec![None, Some(0), Some(1), None, None, Some(2), Some(3), None, Some(4)]);
    }
}
