//! `validate` subcommand: read a plan YAML, check each resource against the taxonomy,
//! emit a report.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::taxonomy::{Taxonomy, Violation};

/// One resource entry in a plan.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Resource {
    /// A stable identifier for the resource (e.g. `aws/ec2/i-abc123`).
    pub id: String,

    /// Optional kind annotation (e.g. `aws_instance`, `kubernetes_pod`). Informational only.
    #[serde(default)]
    pub kind: Option<String>,

    /// Cloud-side tags (AWS tags, GCP labels, Azure tags).
    #[serde(default)]
    pub tags: HashMap<String, String>,

    /// Kubernetes labels.
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

impl Resource {
    /// Combine `tags` and `labels` into a single dimension map. Where both define
    /// the same key, `labels` wins (the workload's K8s label is more specific
    /// than the underlying node tag).
    pub fn dimensions(&self) -> HashMap<String, String> {
        let mut combined = self.tags.clone();
        for (k, v) in &self.labels {
            combined.insert(k.clone(), v.clone());
        }
        combined
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    #[serde(default)]
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub plan_path: String,
    pub total_resources: usize,
    pub clean_resources: usize,
    pub violations: Vec<Violation>,
}

impl Report {
    pub fn render_human(&self) -> String {
        let mut lines = vec![
            format!("plan: {}", self.plan_path),
            format!(
                "resources: {} total, {} clean, {} with violations",
                self.total_resources,
                self.clean_resources,
                self.total_resources.saturating_sub(self.clean_resources),
            ),
        ];
        if self.violations.is_empty() {
            lines.push("\u{2713} all resources pass taxonomy".to_string());
        } else {
            lines.push(format!("\u{2717} {} violations:", self.violations.len()));
            for v in &self.violations {
                match &v.kind {
                    crate::taxonomy::ViolationKind::Missing => {
                        lines.push(format!("  - {}: missing `{}`", v.resource_id, v.dimension));
                    }
                    crate::taxonomy::ViolationKind::Invalid { value, reason } => {
                        lines.push(format!(
                            "  - {}: invalid `{}={}` ({})",
                            v.resource_id, v.dimension, value, reason
                        ));
                    }
                }
            }
        }
        lines.join("\n")
    }
}

pub fn run(taxonomy: &Taxonomy, plan_path: &Path) -> Result<Report> {
    let raw = std::fs::read_to_string(plan_path)
        .with_context(|| format!("reading plan from {}", plan_path.display()))?;
    let plan: Plan = serde_yaml::from_str(&raw)
        .with_context(|| format!("parsing plan YAML at {}", plan_path.display()))?;

    let mut violations = vec![];
    let mut clean = 0;
    for r in &plan.resources {
        let dims = r.dimensions();
        let v = taxonomy.validate_resource(&r.id, &dims);
        if v.is_empty() {
            clean += 1;
        } else {
            violations.extend(v);
        }
    }
    Ok(Report {
        plan_path: plan_path.display().to_string(),
        total_resources: plan.resources.len(),
        clean_resources: clean,
        violations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::Validator;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn small_taxonomy() -> Taxonomy {
        let mut validators = HashMap::new();
        validators.insert(
            "cost_center".to_string(),
            Validator::String {
                regex: Some("^cc-[a-z]+$".into()),
            },
        );
        Taxonomy {
            required: vec!["cost_center".into()],
            optional: vec![],
            validators,
            virtual_allocation: HashMap::new(),
        }
    }

    fn write_plan(yaml: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        f
    }

    #[test]
    fn clean_plan_passes() {
        let plan = write_plan(
            r#"
resources:
  - id: aws/ec2/a
    tags:
      cost_center: cc-foo
  - id: aws/ec2/b
    tags:
      cost_center: cc-bar
"#,
        );
        let r = run(&small_taxonomy(), plan.path()).unwrap();
        assert_eq!(r.total_resources, 2);
        assert_eq!(r.clean_resources, 2);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn missing_tag_detected() {
        let plan = write_plan(
            r#"
resources:
  - id: aws/ec2/a
    tags: {}
"#,
        );
        let r = run(&small_taxonomy(), plan.path()).unwrap();
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].dimension, "cost_center");
    }

    #[test]
    fn k8s_labels_override_node_tags() {
        // Tag has a value that FAILS the regex (underscore where regex demands `[a-z]+`).
        // Label has a value that PASSES the regex.
        // If labels correctly override tags, validation passes.
        let plan = write_plan(
            r#"
resources:
  - id: k8s/pod/foo
    tags:
      cost_center: cc_bad_underscore
    labels:
      cost_center: cc-good
"#,
        );
        let r = run(&small_taxonomy(), plan.path()).unwrap();
        assert!(
            r.violations.is_empty(),
            "labels should override tags; got violations: {:?}",
            r.violations
        );
    }
}
