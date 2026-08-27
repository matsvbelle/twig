//! CLion project colour (the coloured circle / toolbar tint) as a REPO identity:
//! initialised once in the main repo, mirrored verbatim into every worktree.
use crate::error::Result;
use crate::root::name_of;
use crate::{idea, out};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const COMPONENT: &str = "ProjectColorInfo";
/// Size of the IDE's built-in project colour palette.
const PALETTE: usize = 9;

/// The full `<component name="ProjectColorInfo">…</component>` block (with
/// leading indentation and trailing newline) from a workspace.xml, if any.
pub fn get_component(xml: &str) -> Option<String> {
    let (mut s, mut e) = idea::find_component(xml, COMPONENT)?;
    while s > 0 && matches!(xml.as_bytes()[s - 1], b' ' | b'\t') {
        s -= 1;
    }
    if xml[e..].starts_with('\n') {
        e += 1;
    }
    Some(xml[s..e].to_string())
}

/// Palette index from the component's JSON payload (escaped or CDATA form).
pub fn palette_index(component: &str) -> Option<i64> {
    let pos = component.find("associatedIndex")? + "associatedIndex".len();
    let rest = component[pos..].split_once(':')?.1.trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn read_component(ws: &Path) -> Option<String> {
    fs::read_to_string(ws).ok().and_then(|xml| get_component(&xml))
}

fn write_component(ws: &Path, component: &str) -> Result<()> {
    idea::ensure_workspace(ws)?;
    let xml = fs::read_to_string(ws)?;
    let updated = match get_component(&xml) {
        Some(old) => xml.replacen(&old, component, 1),
        None => idea::set_component(&xml, COMPONENT, component),
    };
    fs::write(ws, updated)?;
    Ok(())
}

/// Palette indices used by the other repos.
pub fn used_indices(repos: &[PathBuf], exclude: &Path) -> HashSet<usize> {
    repos
        .iter()
        .filter(|p| p.as_path() != exclude)
        .filter_map(|p| read_component(&p.join(".idea/workspace.xml")))
        .filter_map(|c| palette_index(&c))
        .map(|i| i.rem_euclid(PALETTE as i64) as usize)
        .collect()
}

/// Deterministic per repo name, probing upward past indices siblings use.
pub fn pick_index(repo: &str, used: &HashSet<usize>) -> usize {
    let digest = Sha1::digest(repo.as_bytes());
    let start = digest.iter().fold(0usize, |acc, b| (acc * 256 + *b as usize) % PALETTE);
    (0..PALETTE).map(|off| (start + off) % PALETTE).find(|i| !used.contains(i)).unwrap_or(start)
}

/// Ensure the main repo has a colour (distinct from the other `repos`) and copy
/// it into `wt` (if given).
pub fn apply(main: &Path, repos: &[PathBuf], wt: Option<&Path>) -> Result<()> {
    let repo = name_of(main);
    let main_ws = main.join(".idea/workspace.xml");
    let component = match read_component(&main_ws) {
        Some(c) => c,
        None => {
            let index = pick_index(&repo, &used_indices(repos, main));
            let c = format!(
                "  <component name=\"{COMPONENT}\">{{\n  &quot;associatedIndex&quot;: {index}\n}}</component>\n"
            );
            write_component(&main_ws, &c)?;
            out::say(format!("Project color: initialized palette index {index} in {repo}"));
            c
        }
    };
    if let Some(wt) = wt {
        write_component(&wt.join(".idea/workspace.xml"), &component)?;
        let shown = match palette_index(&component) {
            Some(i) => format!("palette index {i}"),
            None => "custom color".to_string(),
        };
        out::say(format!("Project color: worktree matches {repo} ({shown})"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_parsing_both_forms() {
        assert_eq!(palette_index("<component name=\"ProjectColorInfo\">{\n  &quot;associatedIndex&quot;: 4\n}</component>"), Some(4));
        assert_eq!(palette_index("<![CDATA[{\"associatedIndex\": -1}]]>"), Some(-1));
        assert_eq!(palette_index("{\"customColor\": \"ff0000\"}"), None);
    }

    #[test]
    fn pick_is_deterministic_and_avoids_used() {
        assert_eq!(pick_index("alpha", &HashSet::new()), 4);
        assert_eq!(pick_index("beta", &HashSet::new()), 1);
        assert_eq!(pick_index("beta", &HashSet::from([1])), 2);
        assert_eq!(pick_index("beta", &(0..9).collect()), 1);
    }

    #[test]
    fn apply_initialises_main_and_mirrors_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("alpha");
        let sibling = tmp.path().join("other");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(sibling.join(".idea")).unwrap();
        fs::write(
            sibling.join(".idea/workspace.xml"),
            "<project version=\"4\">\n  <component name=\"ProjectColorInfo\">{\n  &quot;associatedIndex&quot;: 4\n}</component>\n</project>\n",
        )
        .unwrap();
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&wt).unwrap();
        let repos = vec![main.clone(), sibling.clone()];
        apply(&main, &repos, Some(&wt)).unwrap();
        let m = read_component(&main.join(".idea/workspace.xml")).unwrap();
        assert_eq!(palette_index(&m), Some(5), "4 is taken by the sibling");
        assert_eq!(read_component(&wt.join(".idea/workspace.xml")).unwrap(), m);
        // Second run keeps the existing colour.
        apply(&main, &repos, Some(&wt)).unwrap();
        assert_eq!(read_component(&main.join(".idea/workspace.xml")).unwrap(), m);
    }
}
