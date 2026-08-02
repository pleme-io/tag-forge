//! The taxonomy itself: required + optional dimensions, validators per dimension,
//! and virtual-allocation rules for untaggable spend.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Taxonomy {
    /// Required dimensions — a resource missing any of these is in violation.
    #[serde(default)]
    pub required: Vec<String>,

    /// Optional dimensions — present-or-absent is recorded but not a violation.
    #[serde(default)]
    pub optional: Vec<String>,

    /// Validators keyed by dimension name. A dimension without a validator
    /// accepts any non-empty value.
    #[serde(default)]
    pub validators: HashMap<String, Validator>,

    /// Rules for allocating untaggable spend to dimensions.
    #[serde(default)]
    pub virtual_allocation: HashMap<String, AllocationRule>,
}

/// A validator constrains the value space of one dimension.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Validator {
    /// A string, optionally constrained by a regex.
    String {
        #[serde(default)]
        regex: Option<String>,
    },
    /// A value drawn from an enum.
    Enum { values: Vec<String> },
}

/// A virtual-allocation rule maps untaggable spend onto dimensions.
/// The `kind` field names the rule; extra YAML keys flow into `params`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AllocationRule {
    #[serde(rename = "rule")]
    pub kind: String,
    #[serde(flatten)]
    pub params: HashMap<String, serde_yaml::Value>,
}

/// A violation of the taxonomy by a single resource.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub resource_id: String,
    pub dimension: String,
    pub kind: ViolationKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViolationKind {
    Missing,
    Invalid { value: String, reason: String },
}

impl Taxonomy {
    /// Merge `other` on top of `self`. Per-dimension lists are union'd; per-key maps
    /// take `other`'s value where both exist.
    #[must_use]
    pub fn merged_with(&self, other: &Taxonomy) -> Taxonomy {
        let mut required: Vec<String> = self.required.clone();
        for d in &other.required {
            if !required.contains(d) {
                required.push(d.clone());
            }
        }
        let mut optional: Vec<String> = self.optional.clone();
        for d in &other.optional {
            if !optional.contains(d) {
                optional.push(d.clone());
            }
        }
        let mut validators = self.validators.clone();
        for (k, v) in &other.validators {
            validators.insert(k.clone(), v.clone());
        }
        let mut virtual_allocation = self.virtual_allocation.clone();
        for (k, v) in &other.virtual_allocation {
            virtual_allocation.insert(k.clone(), v.clone());
        }
        Taxonomy {
            required,
            optional,
            validators,
            virtual_allocation,
        }
    }

    /// Return all known dimensions (required ∪ optional), preserving order.
    #[must_use]
    pub fn dimensions(&self) -> Vec<&str> {
        let mut all: Vec<&str> = self.required.iter().map(String::as_str).collect();
        for d in &self.optional {
            let s: &str = d.as_str();
            if !all.contains(&s) {
                all.push(s);
            }
        }
        all
    }

    pub fn describe(&self, dimension: &str) -> Option<String> {
        if !self.dimensions().contains(&dimension) {
            return None;
        }
        let req = if self.required.iter().any(|d| d == dimension) {
            "required"
        } else {
            "optional"
        };
        let validator = self
            .validators
            .get(dimension)
            .map_or_else(|| "(no validator)".to_string(), format_validator);
        Some(format!("{dimension} ({req}) — {validator}"))
    }

    pub fn render(&self) -> String {
        let mut lines = vec![
            "taxonomy:".to_string(),
            format!("  required: {:?}", self.required),
            format!("  optional: {:?}", self.optional),
            "  validators:".to_string(),
        ];
        let mut keys: Vec<&String> = self.validators.keys().collect();
        keys.sort();
        for k in keys {
            lines.push(format!("    {}: {}", k, format_validator(&self.validators[k])));
        }
        if !self.virtual_allocation.is_empty() {
            lines.push("  virtual_allocation:".to_string());
            let mut keys: Vec<&String> = self.virtual_allocation.keys().collect();
            keys.sort();
            for k in keys {
                lines.push(format!("    {}: {}", k, self.virtual_allocation[k].kind));
            }
        }
        lines.join("\n")
    }

    /// Validate a single resource's combined dimensions (cloud tags ∪ K8s labels).
    /// Returns a (possibly empty) list of violations.
    pub fn validate_resource(
        &self,
        resource_id: &str,
        dims: &HashMap<String, String>,
    ) -> Vec<Violation> {
        let mut out = vec![];
        for req in &self.required {
            if !dims.contains_key(req) {
                out.push(Violation {
                    resource_id: resource_id.to_string(),
                    dimension: req.clone(),
                    kind: ViolationKind::Missing,
                });
            }
        }
        for (dim, value) in dims {
            if let Some(v) = self.validators.get(dim)
                && let Err(reason) = check_validator(v, value)
            {
                out.push(Violation {
                    resource_id: resource_id.to_string(),
                    dimension: dim.clone(),
                    kind: ViolationKind::Invalid {
                        value: value.clone(),
                        reason,
                    },
                });
            }
        }
        out
    }
}

fn format_validator(v: &Validator) -> String {
    match v {
        Validator::String { regex: Some(r) } => format!("string ~ /{r}/"),
        Validator::String { regex: None } => "string".to_string(),
        Validator::Enum { values } => format!("enum {{{}}}", values.join(", ")),
    }
}

fn check_validator(v: &Validator, value: &str) -> Result<(), String> {
    match v {
        Validator::String { regex: Some(r) } => {
            let re = regex::Regex::new(r).map_err(|e| format!("bad regex `{r}`: {e}"))?;
            if re.is_match(value) {
                Ok(())
            } else {
                Err(format!("does not match /{r}/"))
            }
        }
        Validator::String { regex: None } => Ok(()),
        Validator::Enum { values } => {
            if values.iter().any(|v| v == value) {
                Ok(())
            } else {
                Err(format!("not one of [{}]", values.join(", ")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Taxonomy {
        let mut validators = HashMap::new();
        validators.insert(
            "cost_center".to_string(),
            Validator::String {
                regex: Some("^cc-[a-z]+$".into()),
            },
        );
        validators.insert(
            "environment".to_string(),
            Validator::Enum {
                values: vec!["dev".into(), "prod".into()],
            },
        );
        Taxonomy {
            required: vec!["cost_center".into(), "environment".into()],
            optional: vec!["owner".into()],
            validators,
            virtual_allocation: HashMap::new(),
        }
    }

    #[test]
    fn missing_required_tag_violates() {
        let t = t();
        let mut dims = HashMap::new();
        dims.insert("environment".into(), "prod".into());
        let v = t.validate_resource("r1", &dims);
        assert_eq!(v.len(), 1);
        assert!(matches!(v[0].kind, ViolationKind::Missing));
        assert_eq!(v[0].dimension, "cost_center");
    }

    #[test]
    fn invalid_regex_violates() {
        let t = t();
        let mut dims = HashMap::new();
        dims.insert("cost_center".into(), "wrongformat".into());
        dims.insert("environment".into(), "prod".into());
        let v = t.validate_resource("r1", &dims);
        assert_eq!(v.len(), 1);
        assert!(matches!(v[0].kind, ViolationKind::Invalid { .. }));
    }

    #[test]
    fn invalid_enum_violates() {
        let t = t();
        let mut dims = HashMap::new();
        dims.insert("cost_center".into(), "cc-foo".into());
        dims.insert("environment".into(), "qa".into());
        let v = t.validate_resource("r1", &dims);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].dimension, "environment");
    }

    #[test]
    fn valid_resource_no_violations() {
        let t = t();
        let mut dims = HashMap::new();
        dims.insert("cost_center".into(), "cc-foo".into());
        dims.insert("environment".into(), "prod".into());
        let v = t.validate_resource("r1", &dims);
        assert!(v.is_empty());
    }

    #[test]
    fn merge_unions_required_dims() {
        let base = Taxonomy {
            required: vec!["a".into()],
            ..Default::default()
        };
        let overlay = Taxonomy {
            required: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let merged = base.merged_with(&overlay);
        assert_eq!(merged.required, vec!["a", "b"]);
    }

    #[test]
    fn describe_unknown_dimension_returns_none() {
        assert!(t().describe("nope").is_none());
    }
}
