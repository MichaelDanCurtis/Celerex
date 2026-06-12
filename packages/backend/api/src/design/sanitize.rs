//! Focused HTML sanitizer for ingested design templates (v0.1).
//!
//! The template is stored, later re-rendered (slots filled), and shipped into
//! emails/documents — so it must not carry active content. This strips the
//! high-risk vectors while PRESERVING styling (classes, inline styles, layout)
//! so designs survive intact:
//!   * `<script>…</script>` blocks (and bare `<script .../>`)
//!   * `on*=` event-handler attributes (onclick, onload, …)
//!   * `javascript:` / `vbscript:` URI schemes (neutralized)
//!
//! v0.1 is intentionally a targeted strip (no allowlist) because bundles come
//! from our own desktop app, not arbitrary internet users. Before accepting
//! third-party bundles, upgrade to a real allowlist sanitizer (e.g. `ammonia`)
//! configured to keep design CSS.

/// Returns the cleaned HTML and whether anything was changed.
pub fn sanitize_html(input: &str) -> (String, bool) {
    let mut s = strip_script_blocks(input);
    s = strip_event_handler_attrs(&s);
    s = neutralize_dangerous_schemes(&s);
    let changed = s != input;
    (s, changed)
}

/// Remove `<script …>…</script>` (case-insensitive), including unclosed trailing
/// `<script` (drop to end). Linear scan, no regex.
fn strip_script_blocks(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if let Some(rel) = lower[i..].find("<script") {
            let start = i + rel;
            out.push_str(&input[i..start]);
            // Find the matching </script>, else drop the rest.
            match lower[start..].find("</script>") {
                Some(endrel) => {
                    i = start + endrel + "</script>".len();
                }
                None => break, // unterminated script → drop everything after it
            }
        } else {
            out.push_str(&input[i..]);
            break;
        }
    }
    out
}

/// Strip `on<word>="…"` / `on<word>='…'` / `on<word>=value` event-handler attrs.
fn strip_event_handler_attrs(input: &str) -> String {
    let bytes = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    let lb = lower.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        // Match a boundary `on` that begins an attribute: preceded by whitespace.
        if i + 2 < bytes.len()
            && lb[i] == b'o'
            && lb[i + 1] == b'n'
            && (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && lb[i + 2].is_ascii_alphabetic()
        {
            // Consume on<word>
            let mut j = i + 2;
            while j < bytes.len() && lb[j].is_ascii_alphanumeric() {
                j += 1;
            }
            // Optional whitespace then '='
            let mut k = j;
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            if k < bytes.len() && bytes[k] == b'=' {
                k += 1;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                // Consume the value: quoted or bare.
                if k < bytes.len() && (bytes[k] == b'"' || bytes[k] == b'\'') {
                    let q = bytes[k];
                    k += 1;
                    while k < bytes.len() && bytes[k] != q {
                        k += 1;
                    }
                    if k < bytes.len() {
                        k += 1; // closing quote
                    }
                } else {
                    while k < bytes.len() && !bytes[k].is_ascii_whitespace() && bytes[k] != b'>' {
                        k += 1;
                    }
                }
                // Drop a leading space we left behind, if any.
                if out.ends_with(' ') {
                    out.pop();
                }
                i = k;
                continue;
            }
            // Not an attribute — emit the `on` literally.
            out.push_str(&input[i..j]);
            i = j;
            continue;
        }
        // Push one char (respecting UTF-8 boundaries).
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn neutralize_dangerous_schemes(input: &str) -> String {
    // Replace the scheme so `href="javascript:…"` becomes inert.
    input
        .replace("javascript:", "blocked:")
        .replace("JavaScript:", "blocked:")
        .replace("vbscript:", "blocked:")
}

fn utf8_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte >> 5 == 0b110 {
        2
    } else if first_byte >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_blocks() {
        let (out, changed) = sanitize_html("<h1>Hi</h1><script>alert(1)</script><p>ok</p>");
        assert_eq!(out, "<h1>Hi</h1><p>ok</p>");
        assert!(changed);
    }

    #[test]
    fn drops_unterminated_script() {
        let (out, _) = sanitize_html("<p>ok</p><script>evil");
        assert_eq!(out, "<p>ok</p>");
    }

    #[test]
    fn strips_event_handlers_keeps_other_attrs() {
        let (out, changed) =
            sanitize_html(r#"<a href="/x" onclick="steal()" class="btn">go</a>"#);
        assert_eq!(out, r#"<a href="/x" class="btn">go</a>"#);
        assert!(changed);
    }

    #[test]
    fn neutralizes_javascript_url() {
        let (out, _) = sanitize_html(r#"<a href="javascript:alert(1)">x</a>"#);
        assert!(!out.contains("javascript:"));
        assert!(out.contains("blocked:"));
    }

    #[test]
    fn preserves_clean_design_html() {
        let html = r#"<div class="hero" style="color:#fff"><h1>{{slot:title}}</h1></div>"#;
        let (out, changed) = sanitize_html(html);
        assert_eq!(out, html);
        assert!(!changed);
    }

    #[test]
    fn does_not_mangle_words_starting_with_on() {
        let (out, _) = sanitize_html("<p>online only</p>");
        assert_eq!(out, "<p>online only</p>");
    }
}
