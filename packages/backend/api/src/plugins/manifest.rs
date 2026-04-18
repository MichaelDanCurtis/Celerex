use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub inbound: Vec<InboundCapability>,
    #[serde(default)]
    pub outbound: Vec<OutboundCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundCapability {
    pub id: String,
    pub kind: InboundKind,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboundKind { Webhook, Poll, Manual }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundCapability {
    pub id: String,
    pub kind: OutboundKind,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundKind { Http, Mcp, Agent, Script }
