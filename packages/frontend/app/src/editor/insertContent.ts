import { Text } from "@blocksuite/affine/store";

/**
 * Insert text content into the current BlockSuite document.
 * Splits by double-newlines into paragraphs. Supports basic markdown:
 * - # headings (h1-h6)
 * - - or * unordered list items
 * - 1. ordered list items
 * - ``` code blocks
 */
export function insertContentIntoDoc(store: any, content: string) {
  if (!store) return;

  const root = store.root;
  if (!root) return;

  // Find the note block
  const noteBlock = root.children?.find(
    (c: any) => c.flavour === "affine:note"
  );
  if (!noteBlock) return;

  const lines = content.split("\n");
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Skip empty lines
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Code block
    if (line.trim().startsWith("```")) {
      const lang = line.trim().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++; // skip closing ```
      store.addBlock(
        "affine:code",
        { language: lang || "plain text", text: new Text(codeLines.join("\n")) },
        noteBlock.id
      );
      continue;
    }

    // Heading
    const headingMatch = line.match(/^(#{1,6})\s+(.+)/);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const headingType = `h${level}` as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
      store.addBlock(
        "affine:paragraph",
        { type: headingType, text: new Text(headingMatch[2]) },
        noteBlock.id
      );
      i++;
      continue;
    }

    // Unordered list
    if (/^[\-\*]\s+/.test(line)) {
      store.addBlock(
        "affine:list",
        { type: "bulleted", text: new Text(line.replace(/^[\-\*]\s+/, "")) },
        noteBlock.id
      );
      i++;
      continue;
    }

    // Ordered list
    if (/^\d+\.\s+/.test(line)) {
      store.addBlock(
        "affine:list",
        { type: "numbered", text: new Text(line.replace(/^\d+\.\s+/, "")) },
        noteBlock.id
      );
      i++;
      continue;
    }

    // Regular paragraph
    store.addBlock(
      "affine:paragraph",
      { type: "text", text: new Text(line) },
      noteBlock.id
    );
    i++;
  }
}

/**
 * Extract all text content from the current document.
 */
export function extractDocText(store: any): string {
  if (!store) return "";

  try {
    const parts: string[] = [];

    function walk(block: any) {
      const text = block?.text?.toString?.();
      if (text) parts.push(text);
      const children = block?.children ?? [];
      for (const child of children) walk(child);
    }

    const note =
      store.doc?.getBlockByFlavour?.("affine:note")?.[0] ??
      store.root?.children?.find((c: any) => c.flavour === "affine:note");

    if (note) walk(note);
    else if (store.root) walk(store.root);

    return parts.join("\n");
  } catch {
    return "";
  }
}
