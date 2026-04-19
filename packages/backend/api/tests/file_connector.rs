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
