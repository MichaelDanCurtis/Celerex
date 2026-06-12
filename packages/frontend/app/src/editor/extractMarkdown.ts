// Serialize a BlockSuite document's note blocks back to Markdown — the inverse
// of `insertContentIntoDoc`. Kept symmetric with that helper (headings, lists,
// code, quotes, paragraphs) since review drafts are LLM-generated markdown of
// exactly those shapes. Defensive against BlockSuite's evolving block-model API
// (everything optional-chained; unknown blocks fall back to their plain text).

interface Segment {
  flavour: string;
  md: string;
}

function blockToSegment(b: any): Segment | null {
  const flavour: string = b?.flavour ?? "";
  const text: string = b?.text?.toString?.() ?? "";
  // BlockSuite 0.22 exposes model props directly (b.type, b.language).
  const type: string = b?.type ?? b?.props?.type ?? "text";

  if (flavour === "affine:paragraph") {
    if (/^h[1-6]$/.test(type)) {
      return { flavour, md: "#".repeat(Number(type[1])) + " " + text };
    }
    if (type === "quote") return { flavour, md: "> " + text };
    // Drop empty text paragraphs (e.g. the editor's placeholder line).
    if (!text.trim()) return null;
    return { flavour, md: text };
  }
  if (flavour === "affine:list") {
    const prefix = type === "numbered" ? "1. " : "- ";
    return { flavour, md: prefix + text };
  }
  if (flavour === "affine:code") {
    const lang: string = b?.language ?? b?.props?.language ?? "";
    return { flavour, md: "```" + lang + "\n" + text + "\n```" };
  }
  // Divider / unknown blocks: emit their text if any.
  if (flavour === "affine:divider") return { flavour, md: "---" };
  return text.trim() ? { flavour, md: text } : null;
}

export function extractMarkdown(store: any): string {
  if (!store) return "";
  try {
    const note =
      store.root?.children?.find((c: any) => c.flavour === "affine:note") ??
      store.doc?.getBlockByFlavour?.("affine:note")?.[0];
    const blocks: any[] = note?.children ?? [];

    const segments: Segment[] = [];
    for (const b of blocks) {
      const seg = blockToSegment(b);
      if (seg) segments.push(seg);
    }

    let out = "";
    for (let i = 0; i < segments.length; i++) {
      if (i > 0) {
        // Consecutive list items stay contiguous; everything else gets a blank
        // line between blocks (standard markdown paragraph separation).
        const bothList =
          segments[i - 1].flavour === "affine:list" &&
          segments[i].flavour === "affine:list";
        out += bothList ? "\n" : "\n\n";
      }
      out += segments[i].md;
    }
    return out;
  } catch {
    return "";
  }
}
