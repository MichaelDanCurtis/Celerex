use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::job::{Step, StepKind};
use serde_json::Value;

pub const GRAPH_SCHEMA: &str = r##"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["nodes", "edges"],
  "properties": {
    "nodes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "type", "config"],
        "properties": {
          "id": { "type": "string", "minLength": 1 },
          "type": { "type": "string", "enum": ["inbound", "action", "filter", "outbound"] },
          "config": { "type": "object" }
        }
      }
    },
    "edges": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["from", "to"],
        "properties": {
          "from": { "type": "string" },
          "to": { "type": "string" }
        }
      }
    }
  }
}"##;

pub fn validate(value: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(GRAPH_SCHEMA).unwrap();
    let compiled = jsonschema::JSONSchema::compile(&schema).map_err(|e| e.to_string())?;
    let errs: Vec<String> = compiled
        .validate(value)
        .err()
        .map(|it| it.map(|e| e.to_string()).collect())
        .unwrap_or_default();
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
}

pub fn compile(graph: &Value) -> Result<Vec<Step>, String> {
    validate(graph)?;
    let nodes = graph["nodes"].as_array().ok_or("nodes not array")?;
    let edges = graph["edges"].as_array().cloned().unwrap_or_default();
    let mut steps = Vec::new();
    for n in nodes {
        let id = n["id"].as_str().ok_or("node.id")?.to_string();
        let ty = n["type"].as_str().ok_or("node.type")?;
        let config = n["config"].clone();
        let depends_on: Vec<String> = edges
            .iter()
            .filter(|e| e["to"].as_str() == Some(&id))
            .filter_map(|e| e["from"].as_str().map(|s| s.to_string()))
            .collect();
        let kind = match ty {
            // A filter node flagged `execution: "review"` is a human-in-the-loop
            // gate (`StepKind::Review`) rather than an expression gate. Reusing
            // the filter category keeps its one-in/one-gated-out canvas semantics.
            "filter" if config.get("execution").and_then(|v| v.as_str()) == Some("review") => {
                StepKind::Review {
                    prompt: config["prompt"]
                        .as_str()
                        .unwrap_or("Review and approve this draft")
                        .into(),
                    content: config.get("content").cloned().unwrap_or(Value::Null),
                }
            }
            "filter" => StepKind::Filter {
                expression: config["expression"].as_str().unwrap_or("").into(),
            },
            "outbound" => StepKind::Outbound {
                execution: execution_from(&config)?,
                config: config.clone(),
                inputs: config["inputs"].clone(),
            },
            _ => StepKind::Tool {
                execution: execution_from(&config)?,
                config: config.clone(),
                inputs: config["inputs"].clone(),
            },
        };
        steps.push(Step {
            id,
            depends_on,
            kind,
        });
    }
    Ok(steps)
}

fn execution_from(config: &Value) -> Result<ExecutionType, String> {
    match config["execution"].as_str().unwrap_or("script") {
        "http" => Ok(ExecutionType::Http),
        "mcp" => Ok(ExecutionType::Mcp),
        "agent" => Ok(ExecutionType::Agent),
        "script" => Ok(ExecutionType::Script),
        "llm" => Ok(ExecutionType::Llm),
        "doc" => Ok(ExecutionType::Doc),
        "input" => Ok(ExecutionType::Input),
        "memory" => Ok(ExecutionType::Memory),
        "galvo" => Ok(ExecutionType::Galvo),
        "m365" => Ok(ExecutionType::M365),
        other => Err(format!("unknown execution type: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_minimal_graph() {
        let g = json!({ "nodes": [{"id":"a","type":"inbound","config":{}}], "edges": [] });
        validate(&g).unwrap();
    }

    #[test]
    fn rejects_missing_nodes() {
        let g = json!({ "edges": [] });
        assert!(validate(&g).is_err());
    }

    #[test]
    fn rejects_unknown_node_type() {
        let g = json!({ "nodes": [{"id":"a","type":"squirrel","config":{}}], "edges": [] });
        assert!(validate(&g).is_err());
    }

    #[test]
    fn review_filter_compiles_to_review_step() {
        let g = json!({
            "nodes": [{
                "id": "r", "type": "filter",
                "config": {
                    "execution": "review",
                    "prompt": "Approve it",
                    "content": "{{steps.x.outputs.text}}"
                }
            }],
            "edges": []
        });
        let steps = compile(&g).unwrap();
        match &steps[0].kind {
            StepKind::Review { prompt, content } => {
                assert_eq!(prompt, "Approve it");
                assert_eq!(content, &json!("{{steps.x.outputs.text}}"));
            }
            other => panic!("expected Review, got {other:?}"),
        }
    }

    #[test]
    fn plain_filter_still_compiles_to_filter() {
        let g = json!({
            "nodes": [{"id":"f","type":"filter","config":{"expression":"true"}}],
            "edges": []
        });
        let steps = compile(&g).unwrap();
        assert!(matches!(steps[0].kind, StepKind::Filter { .. }));
    }
}
