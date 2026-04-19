use celerex_api::plugins::file_connector::{docx::DocxConnector, xlsx::XlsxConnector, FileConnector};
use std::path::PathBuf;

#[tokio::test]
async fn xlsx_extracts_first_sheet_as_markdown_table() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/files/sample.xlsx");
    let c = XlsxConnector::default();
    let md = c.extract_markdown(&fixture).await.unwrap();
    assert!(md.contains("| Metric | Value |"), "has header row\n{md}");
    assert!(md.contains("| --- | --- |"), "has separator row\n{md}");
    assert!(md.contains("| Signups | 42 |"), "has first data row\n{md}");
    assert!(md.contains("| Churn | 3 |"), "has second data row\n{md}");
}

#[tokio::test]
async fn docx_extracts_paragraphs_as_markdown() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/files/sample.docx");
    let c = DocxConnector::default();
    let md = c.extract_markdown(&fixture).await.unwrap();
    assert!(md.contains("# Quarterly Brief"), "has H1\n{md}");
    assert!(md.contains("Revenue up 12%."), "has body text\n{md}");
    assert!(md.contains("- A"), "has bullet A\n{md}");
    assert!(md.contains("- B"), "has bullet B\n{md}");
}

#[tokio::test]
async fn xlsx_tools_include_get_cell_and_list_sheets() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/files/sample.xlsx");
    let c = XlsxConnector::default();
    let tools = c.tool_definitions(&fixture).await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(names.contains(&"list_sheets".into()), "names: {names:?}");
    assert!(names.contains(&"get_cell".into()), "names: {names:?}");

    let out = c
        .invoke_tool(
            &fixture,
            "get_cell",
            serde_json::json!({"sheet": null, "row": 2, "col": 2}),
        )
        .await
        .unwrap();
    assert_eq!(out, serde_json::json!("42"));
}

#[tokio::test]
async fn mock_llm_scripts_tool_use_then_finish() {
    use celerex_orchestrator::llm::mock::MockLlm;
    use celerex_orchestrator::llm::provider::Provider;

    let m = MockLlm::new()
        .use_tool("get_cell", serde_json::json!({"row":2,"col":2}))
        .finish("The signups cell is 42.");
    let first = m
        .complete("m", serde_json::json!([]), serde_json::json!([]))
        .await
        .unwrap();
    assert_eq!(first.raw["stop_reason"], "tool_use");
    let second = m
        .complete("m", serde_json::json!([]), serde_json::json!([]))
        .await
        .unwrap();
    assert_eq!(second.raw["content"][0]["text"], "The signups cell is 42.");
}
