//! Config loading + overlay (`extends:`) resolution.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::taxonomy::Taxonomy;

/// Top-level config file shape.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    /// Optional reference to a base config to extend.
    ///
    /// Two forms accepted today:
    /// - **File path** (relative to this config, or absolute) — fully working.
    /// - **`pleme-io/<tool>#configs/<base>`** — placeholder; returns an error until the
    ///   binary ships with bundled reference configs that this resolver can find.
    #[serde(default)]
    pub extends: Option<String>,

    pub taxonomy: Taxonomy,
}

pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let mut cfg: Config = serde_yaml::from_str(&raw)
        .with_context(|| format!("parsing YAML config at {}", path.display()))?;

    if let Some(extends) = cfg.extends.clone() {
        let base = resolve_extends(&extends, path)?;
        cfg.taxonomy = base.taxonomy.merged_with(&cfg.taxonomy);
    }
    Ok(cfg)
}

fn resolve_extends(extends: &str, from: &Path) -> Result<Config> {
    if extends.starts_with("pleme-io/") {
        anyhow::bail!(
            "package-reference `extends:` form is not yet supported ({extends}). \
             Use a file path for now."
        );
    }
    let candidate = std::path::PathBuf::from(extends);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        from.parent().unwrap_or_else(|| Path::new(".")).join(candidate)
    };
    load(&resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_yaml(contents: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f
    }

    #[test]
    fn loads_minimal_config() {
        let f = write_yaml(
            r#"
taxonomy:
  required: [cost_center]
  validators:
    cost_center:
      type: string
"#,
        );
        let cfg = load(f.path()).expect("config load");
        assert_eq!(cfg.taxonomy.required, vec!["cost_center".to_string()]);
        assert!(cfg.taxonomy.validators.contains_key("cost_center"));
    }

    #[test]
    fn extends_overlays_taxonomy() {
        let base = write_yaml(
            r#"
taxonomy:
  required: [cost_center]
  validators:
    cost_center:
      type: string
"#,
        );
        let overlay_yaml = format!(
            "extends: {}\ntaxonomy:\n  required: [cost_center, tenant]\n  validators:\n    tenant:\n      type: enum\n      values: [tenant-a, tenant-b]\n",
            base.path().display()
        );
        let overlay = write_yaml(&overlay_yaml);
        let cfg = load(overlay.path()).expect("config load");
        assert!(cfg.taxonomy.required.contains(&"tenant".to_string()));
        assert!(cfg.taxonomy.required.contains(&"cost_center".to_string()));
        assert!(cfg.taxonomy.validators.contains_key("cost_center"));
        assert!(cfg.taxonomy.validators.contains_key("tenant"));
    }

    #[test]
    fn package_extends_returns_explicit_error() {
        let f = write_yaml(
            r#"
extends: pleme-io/tag-forge#configs/default
taxonomy:
  required: [cost_center]
"#,
        );
        let err = load(f.path()).expect_err("expected error for unsupported extends form");
        assert!(format!("{err:?}").contains("not yet supported"));
    }
}
