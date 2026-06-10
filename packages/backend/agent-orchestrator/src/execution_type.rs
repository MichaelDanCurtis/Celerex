use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionType {
    Http,
    Mcp,
    Agent,
    Script,
    Llm,
    Doc,
    /// A trigger / seed node: echoes its (templated) inputs downstream so an
    /// entry point produces a payload for the rest of the flow to reference.
    Input,
}
