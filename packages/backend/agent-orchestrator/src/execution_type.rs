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
    /// galvo-memory: read/write the Neo4j-backed agent memory via its sidecar.
    Memory,
    /// Galvo federation: invoke a Galvo agent via its control plane.
    Galvo,
    /// Microsoft 365 / Graph: publish to Copilot connectors + read Graph data.
    M365,
    /// Server-side design-template slot substitution — fills a Celerex Design
    /// template's `{{slot:name}}` placeholders with flow data (see render.rs).
    Render,
    /// Living Documents: fill a design template's Living-Doc slots by running each
    /// slot's server-held instruction as a one-shot completion, persisting the
    /// `last_value`, and emitting a flat `{slot: value}` object the downstream
    /// `render` node consumes as its `data`. Spelled `slot_fill` on the wire.
    #[serde(rename = "slot_fill")]
    SlotFill,
}
