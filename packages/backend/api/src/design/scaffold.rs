//! Celerex Design handoff, piece C: auto-scaffold a delivery flow for a live
//! template. When a `live`-mode bundle is ingested, Celerex stands up a starter
//! project_member graph so the design can be filled + shipped on a recurring
//! basis without any manual wiring — the differentiator: "design once, fill +
//! send weekly". The shape follows the locked contract:
//!
//!   trigger → fill → render → review → deliver
//!
//! - `trigger` (inbound/input): where a schedule fires the flow.
//! - `fill`    (action/input):  placeholder data source the user replaces with
//!                              their real fetch (agent / HTTP / connector).
//! - `render`  (action/render): fills the template's `{{slot:name}}` with the
//!                              filled data — carries the template inline so the
//!                              flow renders standalone.
//! - `review`  (filter/review): human-in-the-loop approval gate.
//! - `deliver` (outbound/doc):  writes the approved HTML out as a document.
//!
//! The graph is intentionally runnable as-is and obviously editable: the `fill`
//! node is the one seam the user is expected to rewire.

use serde_json::{json, Value};
use uuid::Uuid;

/// Build the starter delivery graph for a live template. `slots` is the
/// template's slot schema array; it is embedded into the Render node so the
/// node renders without a round-trip to the Design Library. `template_id` is
/// baked into the `fill` node so the `slot_fill` executor knows which template's
/// Living-Doc slots to fill.
pub fn scaffold_live_flow(
    template_id: Uuid,
    title: &str,
    engine: &str,
    template_html: &str,
    slots: &Value,
) -> Value {
    let slots = if slots.is_array() { slots.clone() } else { json!([]) };
    json!({
        "nodes": [
            {
                "id": "trigger",
                "type": "inbound",
                "config": { "execution": "input" }
            },
            {
                // The Living-Doc fill: runs each slot's server-held instruction as
                // a one-shot completion and emits a flat `{slot: value}` object the
                // Render node consumes as its `data`. The engagement_id is injected
                // by `apply_doc_engagement` at run time (never trusted from here).
                "id": "fill",
                "type": "action",
                "config": {
                    "execution": "slot_fill",
                    "template_id": template_id.to_string()
                }
            },
            {
                "id": "render",
                "type": "action",
                "config": {
                    "execution": "render",
                    "engine": engine,
                    "template_html": template_html,
                    "slots": slots,
                    "inputs": { "data": "{{steps.fill.outputs}}" }
                }
            },
            {
                "id": "review",
                "type": "filter",
                "config": {
                    "execution": "review",
                    "prompt": format!("Approve “{title}” before it ships"),
                    "content": "{{steps.render.outputs.html}}"
                }
            },
            {
                "id": "deliver",
                "type": "outbound",
                "config": {
                    "execution": "doc",
                    "title": title,
                    "inputs": { "markdown": "{{steps.review.outputs.content}}" }
                }
            }
        ],
        "edges": [
            { "from": "trigger", "to": "fill" },
            { "from": "fill", "to": "render" },
            { "from": "render", "to": "review" },
            { "from": "review", "to": "deliver" }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_members::graph;

    #[test]
    fn scaffolded_flow_compiles() {
        let slots = json!([{ "name": "headline", "type": "text", "required": true }]);
        let tid = Uuid::new_v4();
        let g = scaffold_live_flow(tid, "Weekly Digest", "mjml", "<h1>{{slot:headline}}</h1>", &slots);
        // It must satisfy the real graph schema + compile to executable steps.
        graph::validate(&g).expect("scaffold must be a valid graph");
        let steps = graph::compile(&g).expect("scaffold must compile to steps");
        assert_eq!(steps.len(), 5);
        // The render node carries the template inline so it renders standalone.
        let render = g["nodes"].as_array().unwrap()
            .iter().find(|n| n["id"] == "render").unwrap();
        assert_eq!(render["config"]["template_html"], "<h1>{{slot:headline}}</h1>");
        assert_eq!(render["config"]["engine"], "mjml");
        assert_eq!(render["config"]["slots"], slots);
    }

    #[test]
    fn fill_node_is_slot_fill_carrying_template_id() {
        let tid = Uuid::new_v4();
        let g = scaffold_live_flow(tid, "X", "html", "<p>{{slot:body}}</p>", &json!([]));
        let fill = g["nodes"].as_array().unwrap()
            .iter().find(|n| n["id"] == "fill").unwrap();
        assert_eq!(fill["config"]["execution"], "slot_fill");
        assert_eq!(fill["config"]["template_id"], tid.to_string());
        // The render node reads the fill node's whole output as its `data`, which
        // the slot_fill handler emits as a flat `{slot: value}` object.
        let render = g["nodes"].as_array().unwrap()
            .iter().find(|n| n["id"] == "render").unwrap();
        assert_eq!(render["config"]["inputs"]["data"], "{{steps.fill.outputs}}");
    }

    #[test]
    fn review_gate_precedes_delivery() {
        let g = scaffold_live_flow(Uuid::new_v4(), "X", "html", "<p>{{slot:body}}</p>", &json!([]));
        let edges = g["edges"].as_array().unwrap();
        // deliver depends on review (no path that ships before a human approves).
        assert!(edges.iter().any(|e| e["from"] == "review" && e["to"] == "deliver"));
        assert!(edges.iter().any(|e| e["from"] == "render" && e["to"] == "review"));
    }

    #[test]
    fn non_array_slots_default_empty() {
        let g = scaffold_live_flow(Uuid::new_v4(), "X", "html", "<p>x</p>", &Value::Null);
        let render = g["nodes"].as_array().unwrap()
            .iter().find(|n| n["id"] == "render").unwrap();
        assert_eq!(render["config"]["slots"], json!([]));
    }
}
