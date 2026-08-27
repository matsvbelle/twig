//! `twig list`: the generated worktrees as a tree.
use crate::error::Result;
use crate::root::{Root, Worktree};
use crate::status::Status;
use crate::{git, out};

pub fn run(all: bool, long: bool) -> Result<()> {
    let root = Root::discover()?;
    let filter = if all { None } else { Some(current_repo(&root)?) };
    let wts: Vec<Worktree> = root
        .worktrees()
        .into_iter()
        .filter(|w| filter.as_deref().is_none_or(|r| w.repo == r))
        .collect();

    let header = match &filter {
        None => format!("{}  (all repos)", root.config.worktrees),
        Some(r) => format!("{}  (repo: {r})", root.config.worktrees),
    };
    if root.worktrees().is_empty() {
        out::say(format!("No worktrees under {}", root.worktrees_dir().display()));
        return Ok(());
    }
    if wts.is_empty() {
        out::say(header);
        out::say(format!("  (none for {})", filter.unwrap_or_default()));
        return Ok(());
    }
    let current = std::env::current_dir().ok().and_then(|d| git::toplevel(&d));
    let rows: Vec<Row> = wts
        .iter()
        .map(|w| {
            let branch = w.branch().unwrap_or_default();
            let marks = if long && !branch.is_empty() { Status::probe(w, &branch, None).marks() } else { String::new() };
            Row { branch_dir: w.branch_dir.clone(), repo: w.repo.clone(), branch, current: current.as_deref() == Some(w.path.as_path()), marks }
        })
        .collect();
    print!("{}", render_tree(&header, &rows));
    Ok(())
}

/// Name of the main repo for the checkout at the cwd.
pub fn current_repo(root: &Root) -> Result<String> {
    match root.main_repo_at(&std::env::current_dir()?) {
        Ok(main) => Ok(root.repo_name(&main)),
        Err(_) => Err("Not inside a git repo. Use -A to list all repos.".into()),
    }
}

pub struct Row {
    pub branch_dir: String,
    pub repo: String,
    pub branch: String,
    pub current: bool,
    /// Pre-rendered status marks (`Status::marks`), may be empty.
    pub marks: String,
}

/// Rows are grouped by branch dir in input order.
pub fn render_tree(header: &str, rows: &[Row]) -> String {
    let mut groups: Vec<(&str, Vec<&Row>)> = Vec::new();
    for row in rows {
        match groups.last_mut() {
            Some((b, items)) if *b == row.branch_dir => items.push(row),
            _ => groups.push((&row.branch_dir, vec![row])),
        }
    }
    let mut s = format!("{}\n", out::bold(header));
    for (gi, (branch_dir, items)) in groups.iter().enumerate() {
        let last_group = gi + 1 == groups.len();
        let (pre, cont) = if last_group { ("└── ", "    ") } else { ("├── ", "│   ") };
        let here = items.iter().any(|r| r.current);
        let name = format!("{branch_dir}/");
        s.push_str(&format!("{}{}\n", out::dim(pre), if here { out::cyan(&name) } else { name }));
        for (ri, row) in items.iter().enumerate() {
            let rpre = if ri + 1 == items.len() { "└── " } else { "├── " };
            let repo = format!("{:<22}", row.repo);
            let branch = format!("[{}]", row.branch);
            let line = if row.current {
                format!("{} {} {}", out::cyan(&repo), out::dim(&branch), out::green("← here"))
            } else {
                format!("{repo} {}", out::dim(&branch))
            };
            s.push_str(&format!("{}{}{line}{}\n", out::dim(cont), out::dim(rpre), row.marks));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(branch_dir: &str, repo: &str, current: bool, marks: &str) -> Row {
        Row { branch_dir: branch_dir.into(), repo: repo.into(), branch: branch_dir.into(), current, marks: marks.into() }
    }

    #[test]
    fn tree_connectors() {
        let rows = vec![row("b1", "alpha", false, ""), row("b1", "beta", true, " [dirty]"), row("b2", "alpha", false, "")];
        let expected = "\
.WORKTREES  (all repos)
├── b1/
│   ├── alpha                  [b1]
│   └── beta                   [b1] ← here [dirty]
└── b2/
    └── alpha                  [b2]
";
        assert_eq!(render_tree(".WORKTREES  (all repos)", &rows), expected);
    }
}
