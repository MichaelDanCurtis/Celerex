use serde_json::Value;
use std::path::PathBuf;

pub struct StargateFixture {
    root: PathBuf,
}

impl StargateFixture {
    pub fn from_fixture_dir() -> Self {
        Self {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/plugins/stargate"),
        }
    }

    pub async fn invoke(&self, capability: &str, _input: Value) -> Result<Value, String> {
        match capability {
            "stargate.query" => {
                let bytes = tokio::fs::read(self.root.join("canned/query.json"))
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown stargate capability: {other}")),
        }
    }
}
