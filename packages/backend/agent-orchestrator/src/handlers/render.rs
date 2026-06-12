use crate::render::{parse_slots, render_template};
use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Handler for `ExecutionType::Render` — the design-handoff "live template" node.
///
/// Config:
///   * `template_html` (or `template`) — the HTML/MJML template with `{{slot:name}}`.
///   * `slots` — the slots schema array (`[{name,type,required}]`).
///   * `engine` — "html" | "mjml" (recorded in the output `format`).
///
/// Slot data comes from the resolved `inputs` map (keyed by slot name) — a flow
/// wires each slot to upstream output via `{{steps.X.outputs.Y}}` templates, e.g.
/// `inputs: { headline: "{{steps.research.outputs.title}}" }`. If `inputs` has a
/// `data` object it's used as the map; otherwise `inputs` itself is the map.
pub struct RenderHandler;

#[async_trait]
impl Handler for RenderHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let template = config
            .get("template_html")
            .and_then(|v| v.as_str())
            .or_else(|| config.get("template").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                DispatchError::Handler("render: config.template_html is required".into())
            })?;
        let slots = parse_slots(config.get("slots").unwrap_or(&Value::Null));
        let data = inputs
            .get("data")
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or(inputs);
        let html = render_template(template, &slots, &data)
            .map_err(|e| DispatchError::Handler(format!("render: {e}")))?;
        let format = config.get("engine").and_then(|v| v.as_str()).unwrap_or("html");
        Ok(json!({ "html": html, "format": format }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fills_template_from_inputs() {
        let config = json!({
            "template_html": "<h1>{{slot:title}}</h1>",
            "slots": [{ "name": "title", "type": "text" }],
            "engine": "html"
        });
        let out = RenderHandler.invoke(config, json!({ "title": "Weekly Digest" })).await.unwrap();
        assert_eq!(out["html"], "<h1>Weekly Digest</h1>");
        assert_eq!(out["format"], "html");
    }

    #[tokio::test]
    async fn errors_without_template() {
        let err = RenderHandler.invoke(json!({}), json!({})).await.unwrap_err();
        assert!(matches!(err, DispatchError::Handler(_)));
    }
}
