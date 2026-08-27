//! `.twig.toml` — its presence marks a directory as twigged.
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = ".twig.toml";
pub const DEFAULT_WORKTREES: &str = ".WORKTREES";
pub const DEFAULT_IDE: &str = "clion";

fn default_ide() -> String {
    DEFAULT_IDE.to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub worktrees: String,
    /// IDE launcher command; the directory to open is appended as its argument.
    #[serde(default = "default_ide")]
    pub ide: String,
    /// Absent = no editor background tint for new worktrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Tint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tint {
    pub opacity: u32,
    pub saturation: f64,
    pub lightness: f64,
}

impl Default for Tint {
    fn default() -> Self {
        Tint { opacity: 7, saturation: 0.55, lightness: 0.55 }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config { worktrees: DEFAULT_WORKTREES.into(), ide: DEFAULT_IDE.into(), tint: Some(Tint::default()) }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Config, String> {
        toml::from_str(s).map_err(|e| format!("invalid {CONFIG_FILE}: {e}"))
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("config serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tint_optional() {
        let c = Config::default();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        let no_tint = Config { worktrees: "wt".into(), ide: "idea".into(), tint: None };
        let s = no_tint.to_toml();
        assert!(!s.contains("tint"));
        assert_eq!(Config::parse(&s).unwrap(), no_tint);
        let minimal = Config::parse("worktrees = \"x\"").unwrap();
        assert_eq!((minimal.tint, minimal.ide.as_str()), (None, "clion"));
        assert!(Config::parse("nonsense").unwrap_err().contains("invalid .twig.toml"));
        assert!(Config::parse("").is_err(), "worktrees is required");
    }
}
