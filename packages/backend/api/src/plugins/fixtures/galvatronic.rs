use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// Fixture for the Galvatronic plugin — a manufacturing/control-plane
/// integration that exposes HTTP-style request/response capabilities
/// for reading sensor data, listing devices, and sending commands to
/// industrial equipment.
///
/// Mirrors [`WorkIqFixture`] and [`StargateFixture`]: a fixture-dir
/// constructor reads canned JSON from disk, and `invoke` dispatches by
/// capability id.
pub struct GalvatronicFixture {
    root: PathBuf,
    canned_responses: HashMap<String, Value>,
}

impl GalvatronicFixture {
    /// Construct a fixture without any disk-backed canned responses.
    /// Useful for unit tests that want to drive the fixture purely from
    /// in-memory overrides via [`Self::with_canned`].
    pub fn new() -> Self {
        Self {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/plugins/galvatronic"),
            canned_responses: HashMap::new(),
        }
    }

    /// Construct a fixture rooted at
    /// `tests/fixtures/plugins/galvatronic/`. `read_sensor` and
    /// `list_devices` are served from `canned/*.json`. `send_command`
    /// is synthesised on the fly because the response depends on input.
    pub fn from_fixture_dir() -> Self {
        Self::new()
    }

    /// Override a capability's response with an in-memory payload. The
    /// override wins over any on-disk canned file.
    pub fn with_canned(mut self, capability: impl Into<String>, value: Value) -> Self {
        self.canned_responses.insert(capability.into(), value);
        self
    }

    pub async fn invoke(&self, capability: &str, inputs: Value) -> Result<Value, String> {
        if let Some(v) = self.canned_responses.get(capability) {
            return Ok(v.clone());
        }
        match capability {
            "galvatronic.read_sensor" => {
                let bytes = tokio::fs::read(self.root.join("canned/read_sensor.json"))
                    .await
                    .map_err(|e| e.to_string())?;
                let mut payload: Value =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                // If the caller passed a sensor_id, echo it back so the
                // downstream step can correlate the reading.
                if let Some(id) = inputs.get("sensor_id").and_then(|v| v.as_str()) {
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert("sensor_id".into(), Value::String(id.to_string()));
                    }
                }
                Ok(payload)
            }
            "galvatronic.list_devices" => {
                let bytes = tokio::fs::read(self.root.join("canned/list_devices.json"))
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())
            }
            "galvatronic.send_command" => {
                let device_id = inputs
                    .get("device_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "galvatronic.send_command requires device_id".to_string())?;
                let command = inputs
                    .get("command")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "galvatronic.send_command requires command".to_string())?;
                Ok(json!({
                    "accepted": true,
                    "command_id": format!("cmd_{device_id}_{command}"),
                    "device_id": device_id,
                    "command": command,
                }))
            }
            other => Err(format!("unknown galvatronic capability: {other}")),
        }
    }
}

impl Default for GalvatronicFixture {
    fn default() -> Self {
        Self::new()
    }
}
