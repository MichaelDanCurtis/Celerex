use celerex_orchestrator::handlers::script::ScriptHandler;
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::json;

#[tokio::test]
async fn script_handler_runs_command_and_captures_stdout() {
    let h = ScriptHandler;
    let cfg = json!({ "command": "echo", "args": ["hello"] });
    let out = h.invoke(cfg, json!({})).await.unwrap();
    assert_eq!(out["stdout"].as_str().unwrap().trim(), "hello");
    assert_eq!(out["exit_code"], 0);
}
