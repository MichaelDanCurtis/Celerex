use serde_json::Value;
use std::path::PathBuf;

pub struct WorkIqFixture {
    root: PathBuf,
}

impl WorkIqFixture {
    pub fn from_fixture_dir() -> Self {
        Self {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/plugins/workiq"),
        }
    }

    pub async fn invoke(&self, capability: &str, _input: Value) -> Result<Value, String> {
        match capability {
            "workiq.pull_metrics" => {
                let bytes = tokio::fs::read(self.root.join("canned/pull_metrics.json"))
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown workiq capability: {other}")),
        }
    }
}
