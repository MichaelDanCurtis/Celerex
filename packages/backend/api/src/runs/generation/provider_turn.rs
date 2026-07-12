use std::collections::{HashMap, HashSet};

use celerex_orchestrator::llm::Provider;
use serde_json::{Value, json};

use super::tools::ToolSpec;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderTurnResult {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Map each tool name to the OpenAI-compatible charset `[a-zA-Z0-9_-]` keeping an
/// injective alias so distinct real names never collide onto one wire name, and
/// wrap each spec in the `{type:"function",function:{...}}` envelope the provider
/// chat/completions API requires. Port of daemon `buildToolNameAliases`.
pub fn build_tool_aliases(tools: &[ToolSpec]) -> (Value, HashMap<String, String>) {
    let mut to_real: HashMap<String, String> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut wire_tools: Vec<Value> = Vec::with_capacity(tools.len());
    for t in tools {
        let base: String = t
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let mut wire = base.clone();
        let mut n = 1u32;
        while used.contains(&wire)
            && to_real.get(&wire).map(|s| s.as_str()) != Some(t.name.as_str())
        {
            wire = format!("{base}_{n}");
            n += 1;
        }
        used.insert(wire.clone());
        to_real.insert(wire.clone(), t.name.clone());
        wire_tools.push(json!({
            "type": "function",
            "function": { "name": wire, "description": t.description, "parameters": t.parameters }
        }));
    }
    (Value::Array(wire_tools), to_real)
}

fn as_u64(v: Option<&Value>) -> Option<u64> {
    v.and_then(|x| x.as_u64())
}

/// Normalize a provider's native completion into ProviderTurnResult. Handles
/// OpenAI/Copilot (`choices[0].message`) and Anthropic (`content[]` blocks).
/// Port of daemon `normalizeRawCompletion`. Tool-call names are NOT reverse
/// mapped here (that happens in `run_provider_turn`).
pub fn normalize_raw_completion(raw: &Value) -> ProviderTurnResult {
    let mut out = ProviderTurnResult::default();

    // OpenAI / Copilot shape.
    if let Some(message) = raw
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
    {
        if let Some(s) = message.get("content").and_then(|c| c.as_str()) {
            out.text.push_str(s);
        }
        if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = match tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                {
                    Some(s) => serde_json::from_str::<Value>(s).unwrap_or(json!({})),
                    None => json!({}),
                };
                out.tool_calls.push(ToolCall { id, name, args });
            }
        }
    }

    // Anthropic content-block shape.
    if let Some(blocks) = raw.get("content").and_then(|c| c.as_array()) {
        for block in blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                        out.text.push_str(s);
                    }
                }
                Some("tool_use") => {
                    out.tool_calls.push(ToolCall {
                        id: block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        args: block.get("input").cloned().unwrap_or(json!({})),
                    });
                }
                _ => {}
            }
        }
    }

    let usage = raw.get("usage");
    out.input_tokens = as_u64(usage.and_then(|u| u.get("prompt_tokens")))
        .or_else(|| as_u64(usage.and_then(|u| u.get("input_tokens"))));
    out.output_tokens = as_u64(usage.and_then(|u| u.get("completion_tokens")))
        .or_else(|| as_u64(usage.and_then(|u| u.get("output_tokens"))));
    out
}

/// Perform ONE LLM turn against the in-process provider. Builds the wire tool
/// envelope, calls `provider.complete` directly (no HTTP hop), normalizes the
/// raw response, and reverse-maps wire tool names back to real names. On a
/// provider error, returns the error string WITHOUT leaking the raw body.
pub async fn run_provider_turn(
    provider: &dyn Provider,
    model: &str,
    messages: Value,
    tools: &[ToolSpec],
) -> Result<ProviderTurnResult, String> {
    let (wire_tools, to_real) = build_tool_aliases(tools);
    let completion = provider.complete(model, messages, wire_tools).await?;
    let mut result = normalize_raw_completion(&completion.raw);
    for tc in result.tool_calls.iter_mut() {
        if let Some(real) = to_real.get(&tc.name) {
            tc.name = real.clone();
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::generation::tools::ToolSpec;
    use celerex_orchestrator::llm::stub::StubProvider;
    use serde_json::json;

    #[test]
    fn normalizes_openai_text_and_tool_calls() {
        let raw = json!({
            "choices": [{ "message": {
                "content": "building now",
                "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a.html\",\"content\":\"x\"}" } }]
            }}],
            "usage": { "prompt_tokens": 10, "completion_tokens": 4 }
        });
        let out = normalize_raw_completion(&raw);
        assert_eq!(out.text, "building now");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "write_file");
        assert_eq!(
            out.tool_calls[0].args,
            json!({ "path": "a.html", "content": "x" })
        );
        assert_eq!(out.input_tokens, Some(10));
        assert_eq!(out.output_tokens, Some(4));
    }

    #[test]
    fn normalizes_anthropic_content_blocks() {
        let raw = json!({
            "content": [
                { "type": "text", "text": "ok " },
                { "type": "tool_use", "id": "tu_1", "name": "write_file", "input": { "path": "b.html" } }
            ],
            "usage": { "input_tokens": 7, "output_tokens": 2 }
        });
        let out = normalize_raw_completion(&raw);
        assert_eq!(out.text, "ok ");
        assert_eq!(out.tool_calls[0].id, "tu_1");
        assert_eq!(out.tool_calls[0].args, json!({ "path": "b.html" }));
        assert_eq!(out.input_tokens, Some(7));
    }

    #[test]
    fn builds_openai_function_envelope_and_sanitizes_names() {
        let tools = vec![ToolSpec {
            name: "mcp__figma.dev__get_file".into(),
            description: "d".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }];
        let (wire, to_real) = build_tool_aliases(&tools);
        let arr = wire.as_array().unwrap();
        assert_eq!(arr[0]["type"], "function");
        let wire_name = arr[0]["function"]["name"].as_str().unwrap();
        assert!(
            wire_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        );
        assert!(!wire_name.contains('.'));
        assert_eq!(to_real.get(wire_name).unwrap(), "mcp__figma.dev__get_file");
    }

    // Hard-won wire lesson (not in the brief's literal test list — added per
    // task instructions): "reversible, injective per-turn alias map ...
    // collision-cascade must stay injective." Three distinct real names that
    // all sanitize to the same base ("tool.a", "tool!a", "tool@a" -> "tool_a")
    // must still receive three DISTINCT wire names, each reverse-mapping to
    // the correct (and only the correct) real name. A buggy suffix scheme
    // (e.g. one that only checks the immediate next candidate instead of
    // walking past every prior collision) would silently alias two real tools
    // onto one wire name and misroute a tool call.
    #[test]
    fn build_tool_aliases_collision_cascade_stays_injective() {
        let tools = vec![
            ToolSpec {
                name: "tool.a".into(),
                description: "d".into(),
                parameters: json!({}),
            },
            ToolSpec {
                name: "tool!a".into(),
                description: "d".into(),
                parameters: json!({}),
            },
            ToolSpec {
                name: "tool@a".into(),
                description: "d".into(),
                parameters: json!({}),
            },
        ];
        let (wire, to_real) = build_tool_aliases(&tools);
        let arr = wire.as_array().unwrap();
        let wire_names: Vec<String> = arr
            .iter()
            .map(|w| w["function"]["name"].as_str().unwrap().to_string())
            .collect();

        // Injective: three distinct real names must produce three distinct wire names.
        let mut sorted = wire_names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            3,
            "collision cascade produced duplicate wire names: {wire_names:?}"
        );

        // Each wire name reverse-maps back to exactly the real name that produced it.
        assert_eq!(to_real.get(&wire_names[0]).unwrap(), "tool.a");
        assert_eq!(to_real.get(&wire_names[1]).unwrap(), "tool!a");
        assert_eq!(to_real.get(&wire_names[2]).unwrap(), "tool@a");
    }

    #[tokio::test]
    async fn run_turn_reverse_maps_tool_name_to_real() {
        // StubProvider echoes a scripted OpenAI response using the SANITIZED wire
        // name; run_provider_turn must reverse-map it to the real tool name.
        let real = "mcp__figma.dev__get_file";
        let (wire, _to_real) = build_tool_aliases(&[ToolSpec {
            name: real.into(),
            description: "d".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }]);
        let wire_name = wire.as_array().unwrap()[0]["function"]["name"]
            .as_str()
            .unwrap()
            .to_string();
        let provider = StubProvider::new(vec![json!({
            "choices": [{ "message": { "content": serde_json::Value::Null,
                "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": wire_name, "arguments": "{\"path\":\"a\"}" } }] } }]
        })]);
        let tools = vec![ToolSpec {
            name: real.into(),
            description: "d".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }];
        let out = run_provider_turn(&provider, "m", json!([]), &tools)
            .await
            .unwrap();
        assert_eq!(out.tool_calls[0].name, real);
        assert_eq!(out.tool_calls[0].args, json!({ "path": "a" }));
    }
}
