//! System-prompt composition for the server-side design-generation loop.
//! Ported from the daemon's `system-prompt.ts`, extended with the B1 ordered
//! fragment composer (cross-review reconciliation
//! `celerex-design/docs/superpowers/specs/2026-07-13-cross-review-reconciliations.md`):
//! feature-contributed fragments (Brand Kit voice, Cinematic motion tier,
//! Infographic chart-design, ...) are composed into the base prompt in
//! canonical SLOT order, never insertion order.

/// One optional, feature-contributed fragment of the system prompt (Brand Kit
/// brand voice, Cinematic motion tier, Infographic chart-design, ...). Fragments
/// are composed into the base prompt in canonical SLOT order — never insertion
/// order. Origin: cross-review reconciliation B1
/// (`celerex-design/docs/superpowers/specs/2026-07-13-cross-review-reconciliations.md`).
#[derive(Debug, Clone)]
pub struct PromptFragment {
    pub slot: FragmentSlot,
    pub text: String,
}

/// Canonical composition order: Brand -> Motion -> Task (the `Task` slot carries
/// the Infographic chart-design fragment and any task-tail copy). Variants are
/// DECLARED in canonical order, so deriving `Ord` makes a stable sort yield it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FragmentSlot {
    Brand,
    Motion,
    Task,
}

/// Combined character budget across ALL composed fragments (the base prompt is
/// never counted and never truncated). Fragments are appended in slot order until
/// the budget is reached; the fragment that crosses it is truncated on a char
/// boundary (truncate-last) and any later fragments are dropped. Origin: B1.
pub const MAX_PROMPT_FRAGMENT_CHARS: usize = 8_000;

/// Output of the composer: the full prompt plus whether the budget forced
/// truncation, so `run_generation` can emit the paired `prompt-fragment-truncated`
/// status note (a free-form `agent`/`status` label, vocabulary-safe exactly like
/// `artifact-error` / `reached step limit`).
#[derive(Debug, Clone)]
pub struct ComposedPrompt {
    pub text: String,
    pub truncated: bool,
}

/// The daemon `system-prompt.ts` base prompt. The `<script>` and `{{ }}`
/// prohibition is load-bearing: the artifact guard rejects both.
fn base_prompt(tool_names: &[String]) -> String {
    let tools = if tool_names.is_empty() {
        "(none)".to_string()
    } else {
        tool_names.join(", ")
    };
    [
        "You are OmniVyx Design, a build agent. Call the provided tools to build the requested design — do not describe the tools or paste file contents into your chat text.".to_string(),
        format!("Available tools: {tools}."),
        "Build a single self-contained document at index.html using write_file (inline CSS/JS; no external build step). Use read_file / edit_file to iterate on files you have written, and list_files to see what exists.".to_string(),
        "Do not include <script> tags and do not include any {{ }} (double-brace) template sequences anywhere in index.html — the live preview renderer rejects both and the run will produce no visible artifact.".to_string(),
        "When the design is complete and you have no further tool calls to make, stop calling tools and reply with a short completion note. That reply (with no tool calls) ends the run.".to_string(),
        "Never invoke a shell command or any command-line callback convention; the only actions available are the provided tools, invoked directly as tool calls.".to_string(),
    ]
    .join("\n\n")
}

/// Compose the base prompt with the ordered fragments. Fragments are sorted into
/// canonical slot order FIRST (so the output is independent of insertion order),
/// blank fragments are skipped, and each surviving fragment is appended after the
/// existing `---` separator. The combined fragment budget is enforced
/// truncate-last: the fragment that crosses `MAX_PROMPT_FRAGMENT_CHARS` is
/// truncated to the remaining budget and any later fragments are dropped;
/// `truncated` records whether that happened. An EMPTY list returns the base
/// prompt byte-for-byte (today's behavior; the old `None` path).
pub fn compose_system_prompt(
    tool_names: &[String],
    fragments: &[PromptFragment],
) -> ComposedPrompt {
    let mut ordered: Vec<&PromptFragment> = fragments
        .iter()
        .filter(|f| !f.text.trim().is_empty())
        .collect();
    ordered.sort_by_key(|f| f.slot); // stable: same-slot fragments keep input order

    let mut text = base_prompt(tool_names);
    let mut used = 0usize; // fragment chars emitted so far (base excluded)
    let mut truncated = false;
    for frag in ordered {
        if used >= MAX_PROMPT_FRAGMENT_CHARS {
            truncated = true;
            break;
        }
        let remaining = MAX_PROMPT_FRAGMENT_CHARS - used;
        let trimmed = frag.text.trim();
        let piece: String = if trimmed.chars().count() <= remaining {
            trimmed.to_string()
        } else {
            truncated = true;
            trimmed.chars().take(remaining).collect()
        };
        used += piece.chars().count();
        text.push_str("\n\n---\n\n");
        text.push_str(&piece);
    }
    ComposedPrompt { text, truncated }
}

/// Convenience wrapper returning just the composed prompt string (drops the
/// truncation flag). Port of daemon `build_system_prompt`, extended to the B1
/// ordered fragment composer; `build_system_prompt(names, &[])` is byte-identical
/// to the pre-composer `None` result.
pub fn build_system_prompt(tool_names: &[String], fragments: &[PromptFragment]) -> String {
    compose_system_prompt(tool_names, fragments).text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["write_file".to_string(), "read_file".to_string()]
    }

    #[test]
    fn empty_fragment_list_is_byte_identical_to_base_prompt() {
        // The empty composer == today's `None` path: base prompt only, no
        // trailing `---` separator, byte-for-byte.
        assert_eq!(build_system_prompt(&names(), &[]), base_prompt(&names()));
        assert!(!build_system_prompt(&names(), &[]).contains("---"));
    }

    #[test]
    fn fragments_compose_in_canonical_slot_order_regardless_of_insertion_order() {
        // Inserted Task, Brand, Motion; output MUST be Brand -> Motion -> Task.
        let frags = vec![
            PromptFragment {
                slot: FragmentSlot::Task,
                text: "TASKFRAG".into(),
            },
            PromptFragment {
                slot: FragmentSlot::Brand,
                text: "BRANDFRAG".into(),
            },
            PromptFragment {
                slot: FragmentSlot::Motion,
                text: "MOTIONFRAG".into(),
            },
        ];
        let out = build_system_prompt(&names(), &frags);
        let (b, m, t) = (
            out.find("BRANDFRAG").unwrap(),
            out.find("MOTIONFRAG").unwrap(),
            out.find("TASKFRAG").unwrap(),
        );
        assert!(b < m && m < t, "expected Brand < Motion < Task ordering");
        assert_eq!(out.matches("\n\n---\n\n").count(), 3); // one separator per fragment
    }

    #[test]
    fn single_fragment_matches_the_legacy_some_branch_shape() {
        // Parity with the removed `Some(fragment)` arm: base + "\n\n---\n\n" + fragment.
        let frags = vec![PromptFragment {
            slot: FragmentSlot::Task,
            text: "EXTRA".into(),
        }];
        assert_eq!(
            build_system_prompt(&names(), &frags),
            format!("{}\n\n---\n\n{}", base_prompt(&names()), "EXTRA")
        );
    }

    #[test]
    fn budget_truncates_last_fragment_and_flags_truncation() {
        // The combined budget is enforced across ALL fragments; the fragment that
        // crosses MAX_PROMPT_FRAGMENT_CHARS is truncated (truncate-last) and
        // `truncated` is reported so run_generation can emit the status note.
        let big = "x".repeat(MAX_PROMPT_FRAGMENT_CHARS + 500);
        let composed = compose_system_prompt(
            &names(),
            &[PromptFragment {
                slot: FragmentSlot::Brand,
                text: big,
            }],
        );
        assert!(composed.truncated);
        let fragment_chars = composed.text.chars().count()
            - base_prompt(&names()).chars().count()
            - "\n\n---\n\n".chars().count();
        assert_eq!(fragment_chars, MAX_PROMPT_FRAGMENT_CHARS);
    }

    #[test]
    fn under_budget_reports_no_truncation() {
        let composed = compose_system_prompt(
            &names(),
            &[PromptFragment {
                slot: FragmentSlot::Brand,
                text: "short".into(),
            }],
        );
        assert!(!composed.truncated);
    }
}
