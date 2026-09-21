//! JetBrains `.idea` handling: sharing config between a main repo and its
//! worktrees, and editing `workspace.xml` the way the IDE stores its settings.
use crate::error::Result;
use std::fs;
use std::path::Path;

/// Team config shared via symlink; everything else copied per worktree.
const SHARED: [&str; 3] = ["codeStyles", "inspectionProfiles", "dictionaries"];
pub const EMPTY_WORKSPACE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<project version=\"4\">\n</project>\n";

/// Populate `<wt>/.idea` from `<main>/.idea`. Returns false if main has none.
pub fn setup(main: &Path, wt: &Path) -> Result<bool> {
    let src = main.join(".idea");
    if !src.is_dir() {
        return Ok(false);
    }
    let dst = wt.join(".idea");
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(&src)?.flatten() {
        let name = entry.file_name();
        let target = dst.join(&name);
        if SHARED.iter().any(|s| *s == name) {
            std::os::unix::fs::symlink(entry.path(), &target)?;
        } else {
            copy_recursive(&entry.path(), &target)?;
        }
    }
    Ok(true)
}

/// Like `cp -r`: directories recursed, symlinks copied as symlinks.
pub fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dst)?;
    } else if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)?.flatten() {
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        fs::copy(src, dst)?;
    }
    Ok(())
}

pub fn ensure_workspace(ws: &Path) -> Result<()> {
    if !ws.exists() {
        fs::create_dir_all(ws.parent().unwrap())?;
        fs::write(ws, EMPTY_WORKSPACE)?;
    }
    Ok(())
}

/// Byte range of `<component name="NAME">…</component>` (inclusive) in `xml`.
pub fn find_component(xml: &str, name: &str) -> Option<(usize, usize)> {
    let open = format!("<component name=\"{name}\">");
    let start = xml.find(&open)?;
    let close = "</component>";
    let end = start + xml[start..].find(close)? + close.len();
    Some((start, end))
}

/// Replace the component, or append it before `</project>` if absent.
pub fn set_component(xml: &str, name: &str, component: &str) -> String {
    match find_component(xml, name) {
        Some((s, e)) => format!("{}{}{}", &xml[..s], component, &xml[e..]),
        None => match xml.rfind("</project>") {
            Some(p) => format!("{}{}{}", &xml[..p], component, &xml[p..]),
            None => format!("{xml}{component}"),
        },
    }
}

/// Set/remove keys in the `PropertiesComponent` JSON payload (what the IDE's
/// "Set Background Image → This project only" writes), preserving the storage
/// form (CDATA vs entity-escaped) the file already uses.
pub fn set_properties(xml: &str, set: &[(&str, &str)], remove: &[&str]) -> Result<String> {
    let mut data = serde_json::json!({});
    let mut cdata = true;
    let range = find_component(xml, "PropertiesComponent");
    if let Some((s, e)) = range {
        let inner = xml[s..e]
            .trim_start_matches("<component name=\"PropertiesComponent\">")
            .trim_end_matches("</component>")
            .trim();
        let json = match inner.strip_prefix("<![CDATA[").and_then(|i| i.strip_suffix("]]>")) {
            Some(raw) => raw.to_string(),
            None => {
                cdata = false;
                html_unescape(inner)
            }
        };
        if !json.trim().is_empty() {
            data = serde_json::from_str(&json).map_err(|e| format!("PropertiesComponent is not JSON: {e}"))?;
        }
    }
    let map = data
        .as_object_mut()
        .ok_or("PropertiesComponent JSON is not an object")?
        .entry("keyToString")
        .or_insert_with(|| serde_json::json!({}));
    let kt = map.as_object_mut().ok_or("keyToString is not an object")?;
    for (k, v) in set {
        kt.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    for k in remove {
        kt.remove(*k);
    }
    let body = serde_json::to_string_pretty(&data).expect("json");
    let inner = if cdata { format!("<![CDATA[{body}]]>") } else { html_escape(&body) };
    let component = format!("<component name=\"PropertiesComponent\">{inner}</component>");
    Ok(match range {
        Some((s, e)) => format!("{}{}{}", &xml[..s], component, &xml[e..]),
        None => set_component(xml, "PropertiesComponent", &format!("  {component}\n")),
    })
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#x27;")
}

pub fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(xml: &str) -> serde_json::Value {
        let (s, e) = find_component(xml, "PropertiesComponent").unwrap();
        let inner = xml[s..e].trim_start_matches("<component name=\"PropertiesComponent\">").trim_end_matches("</component>");
        let json = match inner.strip_prefix("<![CDATA[") {
            Some(r) => r.strip_suffix("]]>").unwrap().to_string(),
            None => html_unescape(inner),
        };
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn properties_added_to_empty_workspace() {
        let out = set_properties(EMPTY_WORKSPACE, &[("idea.background.editor", "x.png,7,tile")], &[]).unwrap();
        assert!(out.contains("<![CDATA["));
        assert!(out.trim_end().ends_with("</project>"));
        assert_eq!(props(&out)["keyToString"]["idea.background.editor"], "x.png,7,tile");
    }

    #[test]
    fn properties_preserve_escaped_form_and_remove_keys() {
        let xml = format!(
            "<project version=\"4\">\n  <component name=\"PropertiesComponent\">{}</component>\n  <component name=\"Other\">1</component>\n</project>\n",
            html_escape(r#"{"keyToString":{"idea.background.frame":"old","keep":"1"}}"#)
        );
        let out = set_properties(&xml, &[("idea.background.editor", "new")], &["idea.background.frame"]).unwrap();
        assert!(!out.contains("CDATA"));
        assert!(out.contains("<component name=\"Other\">1</component>"));
        let p = props(&out);
        assert_eq!(p["keyToString"]["keep"], "1");
        assert_eq!(p["keyToString"]["idea.background.editor"], "new");
        assert!(p["keyToString"].get("idea.background.frame").is_none());
    }

    #[test]
    fn properties_cdata_with_existing_keys_is_merged() {
        let xml = "<project version=\"4\">\n  <component name=\"PropertiesComponent\"><![CDATA[{\n  \"keyToString\": {\n    \"a\": \"1\"\n  }\n}]]></component>\n</project>\n";
        let out = set_properties(xml, &[("b", "2")], &[]).unwrap();
        assert!(out.contains("<![CDATA["));
        let p = props(&out);
        assert_eq!(p["keyToString"]["a"], "1");
        assert_eq!(p["keyToString"]["b"], "2");
    }

    #[test]
    fn component_replace_and_append() {
        let xml = "<project version=\"4\">\n  <component name=\"A\">1</component>\n</project>\n";
        let r = set_component(xml, "A", "  <component name=\"A\">2</component>");
        assert!(r.contains(">2<") && !r.contains(">1<"));
        let a = set_component(xml, "B", "  <component name=\"B\">3</component>\n");
        assert!(a.contains(">1<") && a.contains(">3<") && a.ends_with("</project>\n"));
    }

    #[test]
    fn setup_symlinks_shared_dirs_and_copies_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        let wt = tmp.path().join("wt");
        fs::create_dir_all(main.join(".idea/codeStyles")).unwrap();
        fs::create_dir_all(main.join(".idea/cmake")).unwrap();
        fs::write(main.join(".idea/cmake/x.xml"), "x").unwrap();
        fs::write(main.join(".idea/workspace.xml"), EMPTY_WORKSPACE).unwrap();
        fs::create_dir(&wt).unwrap();
        assert!(setup(&main, &wt).unwrap());
        assert!(fs::symlink_metadata(wt.join(".idea/codeStyles")).unwrap().file_type().is_symlink());
        assert!(!fs::symlink_metadata(wt.join(".idea/cmake")).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(wt.join(".idea/cmake/x.xml")).unwrap(), "x");
        assert!(!setup(&tmp.path().join("nothing"), &wt).unwrap());
    }
}
