//! Server-side template slot substitution for the design-handoff "live template"
//! mode (see docs/plans/2026-06-12-design-handoff-contract.md).
//!
//! A Celerex Design template is HTML/MJML with `{{slot:name}}` placeholders plus a
//! slot schema. This fills the placeholders with typed, context-escaped data so
//! Celerex can re-render a template every run WITHOUT the Electron design engine —
//! "design the shell once, fill + ship it weekly".
//!
//! Substitution is the only thing that runs server-side: no script execution, no
//! headless browser. Everything is escaped by context to keep injected data safe.

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RenderError {
    #[error("missing required slot `{0}`")]
    MissingRequired(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotType {
    Text,
    RichText,
    Number,
    Url,
    Image,
    Table,
    List,
}

impl SlotType {
    fn parse(s: &str) -> SlotType {
        match s {
            "richtext" => SlotType::RichText,
            "number" => SlotType::Number,
            "url" => SlotType::Url,
            "image" => SlotType::Image,
            "table" => SlotType::Table,
            "list" => SlotType::List,
            _ => SlotType::Text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SlotSpec {
    pub name: String,
    pub ty: SlotType,
    pub required: bool,
}

/// Parse a `slots.schema.json` `slots` array into specs. Unknown/absent fields
/// degrade gracefully (type→text, required→false).
pub fn parse_slots(slots: &Value) -> Vec<SlotSpec> {
    slots
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    let name = s.get("name")?.as_str()?.to_string();
                    let ty = SlotType::parse(s.get("type").and_then(|v| v.as_str()).unwrap_or("text"));
                    let required = s.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                    Some(SlotSpec { name, ty, required })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Fill every `{{slot:name}}` in `template` with the matching value in `data`,
/// rendered + escaped per the slot's type. A required slot with no data errors;
/// an optional one renders empty.
pub fn render_template(
    template: &str,
    slots: &[SlotSpec],
    data: &Value,
) -> Result<String, RenderError> {
    let mut out = template.to_string();
    for spec in slots {
        let placeholder = format!("{{{{slot:{}}}}}", spec.name);
        let value = data.get(&spec.name);
        if value.is_none() && spec.required {
            return Err(RenderError::MissingRequired(spec.name.clone()));
        }
        let rendered = value.map(|v| render_value(spec.ty, v)).unwrap_or_default();
        out = out.replace(&placeholder, &rendered);
    }
    Ok(out)
}

fn render_value(ty: SlotType, v: &Value) -> String {
    match ty {
        SlotType::Text | SlotType::Number => html_escape(&value_to_plain(v)),
        // URLs/images land in attribute context (href/src).
        SlotType::Url | SlotType::Image => attr_escape(&value_to_plain(v)),
        SlotType::RichText => render_markdown(&value_to_plain(v)),
        SlotType::Table => render_table(v),
        SlotType::List => render_list(v),
    }
}

fn value_to_plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Attribute-context escape (href/src). Same entity set as HTML escape — keeps
/// quotes/brackets from breaking out of the attribute, and we never emit a raw
/// `javascript:`/`data:` scheme that wasn't already in the data (callers should
/// validate scheme at ingest; this is the last-line escape).
fn attr_escape(s: &str) -> String {
    html_escape(s.trim())
}

/// Minimal, SAFE markdown for richtext body slots: escapes first, then applies
/// `**bold**`, `*italic*`, blank-line paragraphs and single-newline `<br>`.
/// (Headings/lists belong in the template structure, not the slot.) Upgrade to
/// pulldown-cmark if richer body markdown is needed.
fn render_markdown(s: &str) -> String {
    let escaped = html_escape(s);
    let with_inline = apply_inline(&escaped);
    with_inline
        .split("\n\n")
        .map(|para| format!("<p>{}</p>", para.trim().replace('\n', "<br>")))
        .filter(|p| p != "<p></p>")
        .collect::<Vec<_>>()
        .join("")
}

fn apply_inline(s: &str) -> String {
    // Bold then italic, on already-escaped text. Simple paired-marker passes.
    let bolded = replace_paired(s, "**", "<strong>", "</strong>");
    replace_paired(&bolded, "*", "<em>", "</em>")
}

fn replace_paired(s: &str, marker: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = s.split(marker).collect();
    if parts.len() < 3 {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(if i % 2 == 1 { open } else { close });
        }
        out.push_str(part);
    }
    // Odd number of markers → trailing unmatched marker; restore it literally.
    if parts.len() % 2 == 0 {
        out.push_str(marker);
    }
    out
}

/// Table slot data: `{ "columns": [...], "rows": [[...]] }` or an array of row
/// objects (columns derived from the first row's keys).
fn render_table(v: &Value) -> String {
    let (columns, rows): (Vec<String>, Vec<Vec<String>>) = match v {
        Value::Object(o) if o.contains_key("rows") => {
            let cols: Vec<String> = o
                .get("columns")
                .and_then(|c| c.as_array())
                .map(|a| a.iter().map(|c| value_to_plain(c)).collect())
                .unwrap_or_default();
            let rows: Vec<Vec<String>> = o
                .get("rows")
                .and_then(|r| r.as_array())
                .map(|a| {
                    a.iter()
                        .map(|row| {
                            row.as_array()
                                .map(|cells| cells.iter().map(value_to_plain).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            (cols, rows)
        }
        Value::Array(arr) => {
            let cols: Vec<String> = arr
                .first()
                .and_then(|r| r.as_object())
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            let rows = arr
                .iter()
                .map(|row| cols.iter().map(|c| value_to_plain(row.get(c).unwrap_or(&Value::Null))).collect())
                .collect();
            (cols, rows)
        }
        _ => (Vec::new(), Vec::new()),
    };
    let mut out = String::from("<table>");
    if !columns.is_empty() {
        out.push_str("<thead><tr>");
        for c in &columns {
            out.push_str(&format!("<th>{}</th>", html_escape(c)));
        }
        out.push_str("</tr></thead>");
    }
    out.push_str("<tbody>");
    for row in &rows {
        out.push_str("<tr>");
        for cell in row {
            out.push_str(&format!("<td>{}</td>", html_escape(cell)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    out
}

/// List slot data: an array → `<ul><li>…</li></ul>`.
fn render_list(v: &Value) -> String {
    let items = match v {
        Value::Array(a) => a.iter().map(value_to_plain).collect::<Vec<_>>(),
        _ => return String::new(),
    };
    let mut out = String::from("<ul>");
    for it in items {
        out.push_str(&format!("<li>{}</li>", html_escape(&it)));
    }
    out.push_str("</ul>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn slots(spec: Value) -> Vec<SlotSpec> {
        parse_slots(&spec)
    }

    #[test]
    fn text_slot_is_substituted_and_escaped() {
        let s = slots(json!([{ "name": "headline", "type": "text" }]));
        let out = render_template("<h1>{{slot:headline}}</h1>", &s, &json!({ "headline": "A & B <x>" })).unwrap();
        assert_eq!(out, "<h1>A &amp; B &lt;x&gt;</h1>");
    }

    #[test]
    fn missing_required_errors_optional_empties() {
        let s = slots(json!([{ "name": "a", "type": "text", "required": true }]));
        assert_eq!(
            render_template("{{slot:a}}", &s, &json!({})),
            Err(RenderError::MissingRequired("a".into()))
        );
        let s2 = slots(json!([{ "name": "a", "type": "text" }]));
        assert_eq!(render_template("[{{slot:a}}]", &s2, &json!({})).unwrap(), "[]");
    }

    #[test]
    fn image_and_url_attr_escaped() {
        let s = slots(json!([{ "name": "img", "type": "image" }]));
        let out = render_template("<img src=\"{{slot:img}}\">", &s, &json!({ "img": "https://x/y?a=1&b=2" })).unwrap();
        assert_eq!(out, "<img src=\"https://x/y?a=1&amp;b=2\">");
    }

    #[test]
    fn table_from_columns_and_rows() {
        let s = slots(json!([{ "name": "t", "type": "table" }]));
        let data = json!({ "t": { "columns": ["Metric", "Value"], "rows": [["Revenue", 120], ["Leads", 2]] } });
        let out = render_template("{{slot:t}}", &s, &data).unwrap();
        assert!(out.contains("<th>Metric</th><th>Value</th>"));
        assert!(out.contains("<td>Revenue</td><td>120</td>"));
    }

    #[test]
    fn list_from_array() {
        let s = slots(json!([{ "name": "l", "type": "list" }]));
        let out = render_template("{{slot:l}}", &s, &json!({ "l": ["one", "two"] })).unwrap();
        assert_eq!(out, "<ul><li>one</li><li>two</li></ul>");
    }

    #[test]
    fn richtext_paragraphs_and_bold() {
        let s = slots(json!([{ "name": "body", "type": "richtext" }]));
        let out = render_template("{{slot:body}}", &s, &json!({ "body": "Hello **world**\nline two\n\nNext para" })).unwrap();
        assert_eq!(out, "<p>Hello <strong>world</strong><br>line two</p><p>Next para</p>");
    }

    #[test]
    fn multiple_slots_in_one_template() {
        let s = slots(json!([
            { "name": "title", "type": "text" },
            { "name": "cta", "type": "url" }
        ]));
        let out = render_template(
            "<h1>{{slot:title}}</h1><a href=\"{{slot:cta}}\">go</a>",
            &s,
            &json!({ "title": "Hi", "cta": "https://celerex.app" }),
        )
        .unwrap();
        assert_eq!(out, "<h1>Hi</h1><a href=\"https://celerex.app\">go</a>");
    }

    #[test]
    fn richtext_escapes_injection() {
        let s = slots(json!([{ "name": "b", "type": "richtext" }]));
        let out = render_template("{{slot:b}}", &s, &json!({ "b": "<script>alert(1)</script>" })).unwrap();
        assert!(!out.contains("<script>"));
        assert!(out.contains("&lt;script&gt;"));
    }
}
