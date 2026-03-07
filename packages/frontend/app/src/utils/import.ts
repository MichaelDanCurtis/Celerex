/**
 * Utility to import a Markdown file and parse it into a doc-ready structure.
 */

export interface ParsedBlock {
  type: string;
  text: string;
}

/**
 * Opens a file picker for .md/.markdown/.txt files and reads the selected file.
 * Returns the parsed title and raw content, or null if the user cancels.
 */
export function importMarkdownFile(): Promise<{ title: string; content: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".md,.markdown,.txt";
    input.style.display = "none";

    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        cleanup();
        return;
      }

      const reader = new FileReader();
      reader.onload = () => {
        const content = reader.result as string;
        const title = extractTitle(content, file.name);
        resolve({ title, content });
        cleanup();
      };
      reader.onerror = () => {
        resolve(null);
        cleanup();
      };
      reader.readAsText(file);
    });

    // Handle cancel — the input won't fire "change" if the user cancels,
    // but we can listen for focus returning to the window as a heuristic.
    function handleFocus() {
      // Small delay to let the "change" event fire first if a file was selected
      setTimeout(() => {
        if (!input.files?.length) {
          resolve(null);
          cleanup();
        }
      }, 300);
    }

    function cleanup() {
      window.removeEventListener("focus", handleFocus);
      input.remove();
    }

    window.addEventListener("focus", handleFocus);
    document.body.appendChild(input);
    input.click();
  });
}

/**
 * Extracts a title from the markdown content.
 * Uses the first `# Heading` line if present, otherwise falls back to the
 * filename without its extension.
 */
function extractTitle(content: string, filename: string): string {
  const lines = content.split("\n");
  for (const line of lines) {
    const match = line.match(/^#\s+(.+)/);
    if (match) {
      return match[1].trim();
    }
  }
  // Fallback: filename without extension
  return filename.replace(/\.[^.]+$/, "");
}

/**
 * Simple markdown-to-blocks parser.
 * Converts lines of markdown into an array of block descriptions.
 */
export function parseMarkdownToBlocks(content: string): ParsedBlock[] {
  const lines = content.split("\n");
  const blocks: ParsedBlock[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Code fence
    if (line.trimStart().startsWith("```")) {
      const codeLines: string[] = [];
      i++; // skip opening fence
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      i++; // skip closing fence
      blocks.push({ type: "code", text: codeLines.join("\n") });
      continue;
    }

    // Divider
    if (/^---+\s*$/.test(line)) {
      blocks.push({ type: "divider", text: "" });
      i++;
      continue;
    }

    // Headings
    const h3 = line.match(/^###\s+(.+)/);
    if (h3) {
      blocks.push({ type: "h3", text: h3[1].trim() });
      i++;
      continue;
    }

    const h2 = line.match(/^##\s+(.+)/);
    if (h2) {
      blocks.push({ type: "h2", text: h2[1].trim() });
      i++;
      continue;
    }

    const h1 = line.match(/^#\s+(.+)/);
    if (h1) {
      blocks.push({ type: "h1", text: h1[1].trim() });
      i++;
      continue;
    }

    // Blockquote
    const quote = line.match(/^>\s?(.*)/);
    if (quote) {
      blocks.push({ type: "quote", text: quote[1].trim() });
      i++;
      continue;
    }

    // Bulleted list
    const bulleted = line.match(/^[-*]\s+(.+)/);
    if (bulleted) {
      blocks.push({ type: "bulleted", text: bulleted[1].trim() });
      i++;
      continue;
    }

    // Numbered list
    const numbered = line.match(/^\d+\.\s+(.+)/);
    if (numbered) {
      blocks.push({ type: "numbered", text: numbered[1].trim() });
      i++;
      continue;
    }

    // Skip blank lines
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Everything else is plain text
    blocks.push({ type: "text", text: line });
    i++;
  }

  return blocks;
}
