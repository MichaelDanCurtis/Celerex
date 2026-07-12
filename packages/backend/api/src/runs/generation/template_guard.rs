use std::sync::LazyLock;

use regex::Regex;

/// Rejection reason for an `html_template_v1` artifact. Its `.0` string becomes
/// the `artifact-error` status detail surfaced over SSE.
#[derive(Debug, Clone)]
pub struct TemplateGuardError(pub String);

// Port of daemon EXECUTABLE_TEMPLATE_PATTERNS (live-artifacts/render.ts).
static EXECUTABLE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?i)<\s*script\b").unwrap(),
            "script elements are not supported in live artifact previews",
        ),
        (
            Regex::new(r"(?i)<\s*iframe\b").unwrap(),
            "iframe elements are not supported in live artifact previews",
        ),
        (
            Regex::new(r"(?i)<\s*foreignObject\b").unwrap(),
            "foreignObject is not supported in live artifact previews",
        ),
        (
            Regex::new(r"(?i)\bsrcdoc\s*=").unwrap(),
            "srcdoc attributes are not supported in live artifact previews",
        ),
        (
            Regex::new(r"(?i)[\s/]on[a-z][a-z0-9_-]*\s*=").unwrap(),
            "event handler attributes are not supported in live artifact previews",
        ),
        (
            Regex::new(r#"(?i)(?:href|src|action|formaction)\s*=\s*['"]?\s*javascript\s*:"#)
                .unwrap(),
            "javascript: URLs are not supported in live artifact previews",
        ),
        (
            Regex::new(r"(?i)\bdata-od-(?:html|raw|bind-html)\b").unwrap(),
            "raw HTML insertion directives are not supported",
        ),
    ]
});
static RAW_INTERP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\{[^{}]*\}\}\}|\{\{\s*&[^{}]*\}\}").unwrap());
static INTERP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap());
static TEMPLATE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:data|[A-Za-z_][A-Za-z0-9_]*)(?:\.(?:[A-Za-z_][A-Za-z0-9_-]*|\d+))*$").unwrap()
});

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Validate + render `templateHtml`. `dataJson` is always the empty object for
/// generated designs, so every valid `data.*` binding resolves to `""`. Any
/// executable pattern, raw interpolation, or non-`data` binding is rejected.
pub fn render_html_template_v1(template_html: &str) -> Result<String, TemplateGuardError> {
    for (pattern, message) in EXECUTABLE_PATTERNS.iter() {
        if pattern.is_match(template_html) {
            return Err(TemplateGuardError((*message).to_string()));
        }
    }
    if RAW_INTERP.is_match(template_html) {
        return Err(TemplateGuardError(
            "raw template interpolation is not supported".into(),
        ));
    }
    // Replace each `{{ binding }}` with the escaped scalar, rejecting any
    // binding that is not a `data.` path. dataJson is {} so the value is "".
    let mut err: Option<TemplateGuardError> = None;
    let rendered = INTERP.replace_all(template_html, |caps: &regex::Captures| {
        let binding = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if !TEMPLATE_PATH.is_match(binding) || !binding.starts_with("data") {
            err = Some(TemplateGuardError(format!(
                "invalid template binding path: {binding}"
            )));
            return String::new();
        }
        // data object is empty → scalar value is "" → escaped "".
        escape_html("")
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(rendered.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_html_and_leaves_it_unchanged() {
        let html = "<!doctype html><html><body><h1>Hi</h1></body></html>";
        assert_eq!(render_html_template_v1(html).unwrap(), html);
    }

    #[test]
    fn rejects_script_tag() {
        let err = render_html_template_v1("<h1>hi</h1><script>alert(1)</script>").unwrap_err();
        assert!(err.0.to_lowercase().contains("script"));
    }

    #[test]
    fn rejects_iframe_srcdoc_onhandler_and_javascript_url() {
        assert!(render_html_template_v1("<iframe></iframe>").is_err());
        assert!(render_html_template_v1("<div srcdoc=\"x\"></div>").is_err());
        assert!(render_html_template_v1("<button onclick=\"x()\">b</button>").is_err());
        assert!(render_html_template_v1("<a href=\"javascript:alert(1)\">x</a>").is_err());
    }

    #[test]
    fn rejects_stray_moustache_binding() {
        // Not a `data.` path → rejected, matching the daemon.
        assert!(render_html_template_v1("<p>{{ name }}</p>").is_err());
        // Raw triple-brace interpolation is rejected.
        assert!(render_html_template_v1("<p>{{{ raw }}}</p>").is_err());
    }

    #[test]
    fn data_binding_resolves_to_empty_scalar() {
        // dataJson is always {} for generated designs, so a valid data binding
        // renders to the escaped empty string (accepted, not rejected).
        assert_eq!(
            render_html_template_v1("<title>{{ data.title }}</title>").unwrap(),
            "<title></title>"
        );
    }
}

#[cfg(test)]
mod hardening_tests {
    use super::*;

    #[test]
    fn rejects_slash_separated_on_handler() {
        // Gap 1: `/` (not whitespace) before `on`. The ported `\son...=` pattern
        // misses this; the hardened `[\s/]on...=` catches the `<svg/onload>` class.
        assert!(render_html_template_v1("<svg/onload=alert(1)></svg>").is_err());
        assert!(render_html_template_v1("<img/onerror=alert(1)>").is_err());
        // Whitespace-anchored handlers stay rejected (no regression).
        assert!(render_html_template_v1("<button onclick=\"x()\">b</button>").is_err());
    }

    #[test]
    fn rejects_foreign_object() {
        // The HTML-in-SVG bridge — no legitimate static-chart use.
        assert!(
            render_html_template_v1("<svg><foreignObject><b>x</b></foreignObject></svg>").is_err()
        );
        assert!(render_html_template_v1("<foreignobject/>").is_err()); // case-insensitive
    }

    #[test]
    fn hardening_keeps_parity_for_legit_animated_svg_chart() {
        // Parity invariant: a representative animated inline-SVG chart (SMIL
        // <animate> + <text> labels, no handlers, no foreignObject) is returned
        // byte-for-byte unchanged — output === input for clean templates.
        let chart = "<svg viewBox=\"0 0 100 50\"><rect x=\"0\" y=\"10\" width=\"20\" height=\"40\"><animate attributeName=\"height\" from=\"0\" to=\"40\" dur=\"1s\"/></rect><text x=\"0\" y=\"48\">92%</text></svg>";
        assert_eq!(render_html_template_v1(chart).unwrap(), chart);
    }
}
