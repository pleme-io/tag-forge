//! `scan` subcommand: walk a directory of resource manifests, compute per-dimension
//! tag-coverage.

use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use walkdir::WalkDir;

use crate::taxonomy::Taxonomy;
use crate::validate::Plan;

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub root: String,
    pub files_scanned: usize,
    pub resources_total: usize,
    pub resources_clean: usize,
    pub resources_with_violations: usize,
    pub coverage_pct: f64,
    pub per_dimension: BTreeMap<String, DimensionCoverage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DimensionCoverage {
    pub present: usize,
    pub missing: usize,
    pub coverage_pct: f64,
}

impl ScanReport {
    pub fn render_human(&self) -> String {
        let mut lines = vec![
            format!("root: {}", self.root),
            format!(
                "scanned {} files, {} resources",
                self.files_scanned, self.resources_total
            ),
            format!(
                "{} clean, {} with violations, overall coverage {:.1}%",
                self.resources_clean, self.resources_with_violations, self.coverage_pct
            ),
            "per-dimension coverage:".to_string(),
        ];
        for (dim, cov) in &self.per_dimension {
            lines.push(format!(
                "  {dim:24} present={:5} missing={:5} ({:.1}%)",
                cov.present, cov.missing, cov.coverage_pct
            ));
        }
        lines.join("\n")
    }
}

pub fn run(taxonomy: &Taxonomy, root: &Path) -> Result<ScanReport> {
    let mut files_scanned = 0usize;
    let mut resources_total = 0usize;
    let mut resources_clean = 0usize;
    let mut resources_with_violations = 0usize;

    let mut present_per_dim: HashMap<String, usize> = HashMap::new();
    let mut missing_per_dim: HashMap<String, usize> = HashMap::new();

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_yaml(path) {
            continue;
        }
        files_scanned += 1;

        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping unreadable file");
                continue;
            }
        };

        // Plan-shaped files have a top-level `resources:` array. Anything else is silently skipped.
        let plan: Plan = match serde_yaml::from_str(&raw) {
            Ok(p) => p,
            Err(_) => continue,
        };

        for r in &plan.resources {
            resources_total += 1;
            let dims = r.dimensions();
            let v = taxonomy.validate_resource(&r.id, &dims);

            for req in &taxonomy.required {
                if dims.contains_key(req) {
                    *present_per_dim.entry(req.clone()).or_insert(0) += 1;
                } else {
                    *missing_per_dim.entry(req.clone()).or_insert(0) += 1;
                }
            }
            for opt in &taxonomy.optional {
                if dims.contains_key(opt) {
                    *present_per_dim.entry(opt.clone()).or_insert(0) += 1;
                } else {
                    *missing_per_dim.entry(opt.clone()).or_insert(0) += 1;
                }
            }

            if v.is_empty() {
                resources_clean += 1;
            } else {
                resources_with_violations += 1;
            }
        }
    }

    let mut per_dimension: BTreeMap<String, DimensionCoverage> = BTreeMap::new();
    let total = resources_total as f64;
    for dim in taxonomy.dimensions() {
        let present = *present_per_dim.get(dim).unwrap_or(&0);
        let missing = *missing_per_dim.get(dim).unwrap_or(&0);
        let cov = if total > 0.0 {
            (present as f64) / total * 100.0
        } else {
            0.0
        };
        per_dimension.insert(
            dim.to_string(),
            DimensionCoverage {
                present,
                missing,
                coverage_pct: cov,
            },
        );
    }

    let overall_pct = if resources_total > 0 {
        (resources_clean as f64) / (resources_total as f64) * 100.0
    } else {
        0.0
    };

    Ok(ScanReport {
        root: root.display().to_string(),
        files_scanned,
        resources_total,
        resources_clean,
        resources_with_violations,
        coverage_pct: overall_pct,
        per_dimension,
    })
}

fn is_yaml(p: &Path) -> bool {
    matches!(p.extension().and_then(|s| s.to_str()), Some("yaml" | "yml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::Validator;
    use std::fs;
    use tempfile::TempDir;

    fn small_taxonomy() -> Taxonomy {
        let mut validators = HashMap::new();
        validators.insert(
            "cost_center".to_string(),
            Validator::String { regex: None },
        );
        Taxonomy {
            required: vec!["cost_center".into()],
            optional: vec!["owner".into()],
            validators,
            virtual_allocation: HashMap::new(),
        }
    }

    #[test]
    fn scans_directory_and_aggregates() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("a.yaml"),
            r#"resources:
  - id: r1
    tags: {cost_center: cc-foo, owner: a@b}
  - id: r2
    tags: {cost_center: cc-bar}
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("b.yaml"),
            r#"resources:
  - id: r3
    tags: {owner: c@d}
"#,
        )
        .unwrap();

        let r = run(&small_taxonomy(), dir.path()).unwrap();
        assert_eq!(r.files_scanned, 2);
        assert_eq!(r.resources_total, 3);
        assert_eq!(r.resources_clean, 2);
        assert_eq!(r.resources_with_violations, 1);
        let cc = r.per_dimension.get("cost_center").unwrap();
        assert_eq!(cc.present, 2);
        assert_eq!(cc.missing, 1);
    }

    #[test]
    fn empty_dir_yields_zero_report() {
        let dir = TempDir::new().unwrap();
        let r = run(&small_taxonomy(), dir.path()).unwrap();
        assert_eq!(r.resources_total, 0);
        assert_eq!(r.coverage_pct, 0.0);
    }
}
