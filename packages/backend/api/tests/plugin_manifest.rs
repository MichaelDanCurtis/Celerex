use celerex_api::plugins::manifest::PluginManifest;

const MANIFEST_JSON: &str = r#"{
  "id": "teams",
  "name": "Microsoft Teams",
  "version": "0.1.0",
  "inbound": [
    { "id": "on_meeting_complete", "kind": "webhook",
      "input_schema": { "type": "object", "properties": { "meetingId": { "type": "string" } } } }
  ],
  "outbound": [
    { "id": "post_channel", "kind": "http",
      "input_schema": { "type": "object", "properties": { "channel": { "type": "string" }, "message": { "type": "string" } } } }
  ]
}"#;

#[test]
fn parses_bidirectional_manifest() {
    let m: PluginManifest = serde_json::from_str(MANIFEST_JSON).unwrap();
    assert_eq!(m.id, "teams");
    assert_eq!(m.inbound.len(), 1);
    assert_eq!(m.outbound.len(), 1);
    assert_eq!(m.inbound[0].id, "on_meeting_complete");
    assert_eq!(m.outbound[0].id, "post_channel");
}

#[test]
fn empty_inbound_outbound_arrays_are_explicit() {
    let m: PluginManifest = serde_json::from_str(r#"{"id":"x","name":"X","version":"0","inbound":[],"outbound":[]}"#).unwrap();
    assert!(m.inbound.is_empty());
    assert!(m.outbound.is_empty());
}
