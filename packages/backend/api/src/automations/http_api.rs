use async_trait::async_trait;
use serde_json::Value;
use crate::automations::source::{EventSource, PolledItem};

/// Polls a JSON endpoint. `source_config`: { url, items_path, id_field }. New =
/// item whose `id_field` value (as a string) sorts after `cursor`. Cursor = max id seen.
pub struct HttpApiEventSource {
    /// Optional vaulted api_key for the `http-api` connector; sent as a bearer.
    pub api_key: Option<String>,
}

fn cmp_ids(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i128>(), b.parse::<i128>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

#[async_trait]
impl EventSource for HttpApiEventSource {
    async fn poll(&self, source_config: &Value, cursor: Option<&str>) -> Result<(Vec<PolledItem>, Option<String>), String> {
        let url = source_config.get("url").and_then(|v| v.as_str()).ok_or("source_config.url required")?;
        let items_path = source_config.get("items_path").and_then(|v| v.as_str()).unwrap_or("items");
        let id_field = source_config.get("id_field").and_then(|v| v.as_str()).unwrap_or("id");
        let client = reqwest::Client::new();
        let mut req = client.get(url);
        if let Some(key) = &self.api_key { req = req.bearer_auth(key); }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        let arr = body.get(items_path).and_then(|v| v.as_array()).ok_or(format!("items_path '{items_path}' is not an array"))?;
        let mut new_items = Vec::new();
        let mut high = cursor.map(|s| s.to_string());
        for item in arr {
            let id = match item.get(id_field) {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string().trim_matches('"').to_string(),
                None => continue,
            };
            let is_new = match cursor { Some(c) => cmp_ids(&id, c) == std::cmp::Ordering::Greater, None => true };
            if is_new { new_items.push(PolledItem { id: id.clone(), payload: item.clone() }); }
            high = Some(match high { Some(h) if cmp_ids(&id, &h) != std::cmp::Ordering::Greater => h, _ => id });
        }
        Ok((new_items, high))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    #[tokio::test]
    async fn returns_only_items_past_the_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [ {"id":"1","t":"a"}, {"id":"2","t":"b"}, {"id":"3","t":"c"} ]
            }))).mount(&server).await;
        let src = HttpApiEventSource { api_key: None };
        let cfg = serde_json::json!({ "url": server.uri(), "items_path": "items", "id_field": "id" });
        let (items, cursor) = src.poll(&cfg, Some("1")).await.unwrap();
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["2","3"]);
        assert_eq!(cursor.as_deref(), Some("3"));
    }

    #[tokio::test]
    async fn empty_when_nothing_new() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "items": [{"id":"1"}] })))
            .mount(&server).await;
        let src = HttpApiEventSource { api_key: None };
        let cfg = serde_json::json!({ "url": server.uri(), "items_path": "items", "id_field": "id" });
        let (items, cursor) = src.poll(&cfg, Some("1")).await.unwrap();
        assert!(items.is_empty());
        assert_eq!(cursor.as_deref(), Some("1"));
    }
}
