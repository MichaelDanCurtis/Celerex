//! Minimal template resolver for graph step inputs. Supports a single syntax:
//! `{{steps.<step_id>.outputs.<dotted.path>}}`.
//!
//! Recurses through JSON structure. A string whose entire content is one
//! template is replaced with the typed value the path resolves to (string,
//! object, array, number, bool, null); strings with embedded templates get
//! string interpolation (typed values stringified via JSON). Unknown paths
//! are a hard error — the executor should fail the step rather than silently
//! ship an unresolved `{{...}}` to a handler.

use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("unknown step `{0}` referenced in template")]
    UnknownStep(String),
    #[error("template path does not start with `steps.<id>.outputs`: {0}")]
    BadPath(String),
    #[error("path `{0}` did not resolve against step outputs")]
    MissingPath(String),
}

#[derive(Default, Debug, Clone)]
pub struct StepContext {
    pub outputs: HashMap<String, Value>,
}

impl StepContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_output(mut self, step_id: &str, output: Value) -> Self {
        self.outputs.insert(step_id.to_string(), output);
        self
    }

    pub fn set(&mut self, step_id: &str, output: Value) {
        self.outputs.insert(step_id.to_string(), output);
    }

    /// Look up a step's recorded output, if any. Used by the executor to tell
    /// whether a `Review` step already has a (seeded) human decision — present
    /// means resolve the gate, absent means pause the run.
    pub fn output(&self, step_id: &str) -> Option<&Value> {
        self.outputs.get(step_id)
    }
}

pub fn resolve(value: &Value, ctx: &StepContext) -> Result<Value, TemplateError> {
    match value {
        Value::String(s) => resolve_string(s, ctx),
        Value::Array(a) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                out.push(resolve(v, ctx)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(o) => {
            let mut out = serde_json::Map::with_capacity(o.len());
            for (k, v) in o {
                out.insert(k.clone(), resolve(v, ctx)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// If `s` is exactly one template (`{{...}}`), return the typed resolved value.
/// Otherwise substitute each `{{...}}` with its stringified form and return a
/// `Value::String`. Strings without templates pass through untouched.
fn resolve_string(s: &str, ctx: &StepContext) -> Result<Value, TemplateError> {
    let trimmed = s.trim();
    if let Some(inner) = whole_template(trimmed) {
        return lookup(inner.trim(), ctx);
    }
    if !s.contains("{{") {
        return Ok(Value::String(s.to_string()));
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let close = after_open
            .find("}}")
            .ok_or_else(|| TemplateError::BadPath(s.to_string()))?;
        let expr = after_open[..close].trim();
        let v = lookup(expr, ctx)?;
        match v {
            Value::String(inner) => out.push_str(&inner),
            other => out.push_str(&other.to_string()),
        }
        rest = &after_open[close + 2..];
    }
    out.push_str(rest);
    Ok(Value::String(out))
}

fn whole_template(s: &str) -> Option<&str> {
    let inner = s.strip_prefix("{{")?.strip_suffix("}}")?;
    if inner.contains("{{") || inner.contains("}}") {
        return None;
    }
    Some(inner)
}

fn lookup(expr: &str, ctx: &StepContext) -> Result<Value, TemplateError> {
    // Expected shape: steps.<step_id>.outputs.<dotted.path>
    let mut parts = expr.split('.');
    let (Some("steps"), Some(step_id)) = (parts.next(), parts.next()) else {
        return Err(TemplateError::BadPath(expr.to_string()));
    };
    if parts.next() != Some("outputs") {
        return Err(TemplateError::BadPath(expr.to_string()));
    }
    let root = ctx
        .outputs
        .get(step_id)
        .ok_or_else(|| TemplateError::UnknownStep(step_id.to_string()))?;
    let mut cur = root;
    for seg in parts {
        cur = match cur {
            Value::Object(m) => m
                .get(seg)
                .ok_or_else(|| TemplateError::MissingPath(expr.to_string()))?,
            Value::Array(a) => {
                let idx: usize = seg
                    .parse()
                    .map_err(|_| TemplateError::MissingPath(expr.to_string()))?;
                a.get(idx)
                    .ok_or_else(|| TemplateError::MissingPath(expr.to_string()))?
            }
            _ => return Err(TemplateError::MissingPath(expr.to_string())),
        };
    }
    Ok(cur.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> StepContext {
        StepContext::new().with_output(
            "galvo",
            json!({"result": "# Weekly Digest\n- item", "meta": {"status": "success"}}),
        )
    }

    #[test]
    fn whole_template_returns_typed_value() {
        let v = resolve(&json!("{{steps.galvo.outputs.result}}"), &ctx()).unwrap();
        assert_eq!(v, json!("# Weekly Digest\n- item"));
    }

    #[test]
    fn whole_template_returns_nested_typed_value() {
        let v = resolve(&json!("{{steps.galvo.outputs.meta}}"), &ctx()).unwrap();
        assert_eq!(v, json!({"status": "success"}));
    }

    #[test]
    fn embedded_template_interpolates_as_string() {
        let v = resolve(
            &json!("status={{steps.galvo.outputs.meta.status}}!"),
            &ctx(),
        )
        .unwrap();
        assert_eq!(v, json!("status=success!"));
    }

    #[test]
    fn recurses_into_objects_and_arrays() {
        let input = json!({
            "markdown": "{{steps.galvo.outputs.result}}",
            "tags": ["a", "{{steps.galvo.outputs.meta.status}}"]
        });
        let got = resolve(&input, &ctx()).unwrap();
        assert_eq!(got["markdown"], json!("# Weekly Digest\n- item"));
        assert_eq!(got["tags"], json!(["a", "success"]));
    }

    #[test]
    fn non_template_string_passes_through() {
        let v = resolve(&json!("just a value"), &ctx()).unwrap();
        assert_eq!(v, json!("just a value"));
    }

    #[test]
    fn unknown_step_is_error() {
        let err = resolve(&json!("{{steps.missing.outputs.x}}"), &ctx()).unwrap_err();
        assert!(matches!(err, TemplateError::UnknownStep(_)));
    }

    #[test]
    fn missing_path_is_error() {
        let err = resolve(&json!("{{steps.galvo.outputs.nope}}"), &ctx()).unwrap_err();
        assert!(matches!(err, TemplateError::MissingPath(_)));
    }

    #[test]
    fn bad_path_shape_is_error() {
        let err = resolve(&json!("{{foo.bar}}"), &ctx()).unwrap_err();
        assert!(matches!(err, TemplateError::BadPath(_)));
    }
}
