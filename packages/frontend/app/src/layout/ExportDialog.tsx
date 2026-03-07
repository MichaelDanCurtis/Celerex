import { useAtomValue } from "jotai";
import { editorStoreAtom } from "../state/atoms";
import { exportDocAsMarkdown, downloadAsFile } from "../utils/export";

interface ExportDialogProps {
  open: boolean;
  onClose: () => void;
}

interface FormatOption {
  id: string;
  name: string;
  ext: string;
  description: string;
}

const formats: FormatOption[] = [
  { id: "markdown", name: "Markdown", ext: ".md", description: "Export as Markdown with headings, lists, and code blocks" },
  { id: "html", name: "HTML", ext: ".html", description: "Styled HTML document ready for the browser" },
  { id: "text", name: "Plain Text", ext: ".txt", description: "Raw text content without any formatting" },
  { id: "json", name: "JSON", ext: ".json", description: "Structured data with block types and content" },
];

function blockToHtml(block: any): string | null {
  const flavour: string = block.flavour;
  const yBlock = block.yBlock;

  if (flavour === "affine:divider") return "<hr>";

  const text = yBlock?.get("prop:text")?.toString() ?? "";
  const escaped = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  if (flavour === "affine:paragraph") {
    const type: string = yBlock?.get("prop:type") ?? "text";
    switch (type) {
      case "h1": return `<h1>${escaped}</h1>`;
      case "h2": return `<h2>${escaped}</h2>`;
      case "h3": return `<h3>${escaped}</h3>`;
      case "h4": return `<h4>${escaped}</h4>`;
      case "quote": return `<blockquote>${escaped}</blockquote>`;
      default: return `<p>${escaped}</p>`;
    }
  }

  if (flavour === "affine:list") {
    const type: string = yBlock?.get("prop:type") ?? "bulleted";
    if (type === "numbered") return `<ol><li>${escaped}</li></ol>`;
    return `<ul><li>${escaped}</li></ul>`;
  }

  if (flavour === "affine:code") {
    return `<pre><code>${escaped}</code></pre>`;
  }

  return null;
}

function blockToText(block: any): string | null {
  const flavour: string = block.flavour;
  const yBlock = block.yBlock;

  if (flavour === "affine:divider") return "---";

  const text = yBlock?.get("prop:text")?.toString() ?? "";
  if (flavour === "affine:paragraph" || flavour === "affine:list" || flavour === "affine:code") {
    return text;
  }

  return null;
}

function blockToJsonObj(block: any): object | null {
  const flavour: string = block.flavour;
  const yBlock = block.yBlock;

  if (flavour === "affine:divider") return { type: "divider" };

  const text = yBlock?.get("prop:text")?.toString() ?? "";

  if (flavour === "affine:paragraph") {
    const type: string = yBlock?.get("prop:type") ?? "text";
    return { type, text };
  }

  if (flavour === "affine:list") {
    const type: string = yBlock?.get("prop:type") ?? "bulleted";
    return { type: `list-${type}`, text };
  }

  if (flavour === "affine:code") {
    return { type: "code", text };
  }

  return null;
}

function exportAsHtml(store: any): void {
  const root = store.root;
  if (!root) return;

  const title = root.yBlock?.get("prop:title")?.toString() ?? "Untitled";
  const escapedTitle = title.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const bodyParts: string[] = [];

  const children: any[] = root.children ?? [];
  for (const child of children) {
    const html = blockToHtml(child);
    if (html !== null) bodyParts.push(html);
  }

  const htmlDoc = `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>${escapedTitle}</title>
<style>
  body { max-width: 700px; margin: 40px auto; font-family: Georgia, "Times New Roman", serif; font-size: 14px; line-height: 1.6; color: #222; }
  h1 { font-size: 24px; margin: 24px 0 12px; }
  h2 { font-size: 20px; margin: 20px 0 10px; }
  h3 { font-size: 17px; margin: 18px 0 8px; }
  h4 { font-size: 15px; margin: 16px 0 6px; }
  p { margin: 8px 0; }
  blockquote { margin: 12px 0; padding: 8px 16px; border-left: 3px solid #ccc; color: #555; }
  pre { background: #f5f5f5; padding: 12px; border-radius: 4px; overflow-x: auto; font-size: 13px; }
  code { font-family: "SFMono-Regular", Consolas, monospace; }
  hr { border: none; border-top: 1px solid #ddd; margin: 20px 0; }
  ul, ol { margin: 8px 0; padding-left: 24px; }
</style>
</head>
<body>
<h1>${escapedTitle}</h1>
${bodyParts.join("\n")}
</body>
</html>`;

  const filename = `${title.replace(/[^a-zA-Z0-9_-]/g, "_")}.html`;
  downloadAsFile(htmlDoc, filename);
}

function exportAsText(store: any): void {
  const root = store.root;
  if (!root) return;

  const title = root.yBlock?.get("prop:title")?.toString() ?? "Untitled";
  const lines: string[] = [title];

  const children: any[] = root.children ?? [];
  for (const child of children) {
    const text = blockToText(child);
    if (text !== null) lines.push(text);
  }

  const content = lines.join("\n\n");
  const filename = `${title.replace(/[^a-zA-Z0-9_-]/g, "_")}.txt`;
  downloadAsFile(content, filename);
}

function exportAsJson(store: any): void {
  const root = store.root;
  if (!root) return;

  const title = root.yBlock?.get("prop:title")?.toString() ?? "Untitled";
  const blocks: object[] = [];

  const children: any[] = root.children ?? [];
  for (const child of children) {
    const obj = blockToJsonObj(child);
    if (obj !== null) blocks.push(obj);
  }

  const content = JSON.stringify({ title, blocks }, null, 2);
  const filename = `${title.replace(/[^a-zA-Z0-9_-]/g, "_")}.json`;
  downloadAsFile(content, filename);
}

export function ExportDialog({ open, onClose }: ExportDialogProps) {
  const editorStore = useAtomValue(editorStoreAtom);

  if (!open) return null;

  function handleExport(formatId: string) {
    if (!editorStore) return;
    switch (formatId) {
      case "markdown":
        exportDocAsMarkdown(editorStore);
        break;
      case "html":
        exportAsHtml(editorStore);
        break;
      case "text":
        exportAsText(editorStore);
        break;
      case "json":
        exportAsJson(editorStore);
        break;
    }
    onClose();
  }

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0, 0, 0, 0.4)",
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 520,
          background: "var(--celerex-bg-primary, #fff)",
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.2)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          style={{
            padding: "20px 24px 16px",
            borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 18,
              fontWeight: 600,
              color: "var(--celerex-text-primary, #111)",
            }}
          >
            Export Document
          </h2>
          <button
            onClick={onClose}
            style={{
              background: "none",
              border: "none",
              fontSize: 18,
              cursor: "pointer",
              color: "var(--celerex-text-secondary, #888)",
              padding: "4px 8px",
              borderRadius: 4,
              lineHeight: 1,
            }}
          >
            x
          </button>
        </div>

        {/* Format cards */}
        <div style={{ padding: "16px 24px 24px", display: "flex", flexDirection: "column", gap: 10 }}>
          {!editorStore && (
            <p style={{ color: "var(--celerex-text-secondary, #888)", fontSize: 13, margin: "0 0 8px" }}>
              No document is currently open.
            </p>
          )}
          {formats.map((fmt) => (
            <button
              key={fmt.id}
              onClick={() => handleExport(fmt.id)}
              disabled={!editorStore}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 16,
                padding: "14px 16px",
                borderRadius: 8,
                border: "1px solid var(--celerex-border, #e5e7eb)",
                background: "var(--celerex-bg-secondary, #f9fafb)",
                cursor: editorStore ? "pointer" : "not-allowed",
                opacity: editorStore ? 1 : 0.5,
                textAlign: "left",
                width: "100%",
                transition: "background 0.15s, border-color 0.15s",
              }}
              onMouseEnter={(e) => {
                if (editorStore) {
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--celerex-accent, #2563eb)";
                  (e.currentTarget as HTMLElement).style.background = "var(--celerex-bg-hover, rgba(0,0,0,0.04))";
                }
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.borderColor = "var(--celerex-border, #e5e7eb)";
                (e.currentTarget as HTMLElement).style.background = "var(--celerex-bg-secondary, #f9fafb)";
              }}
            >
              <div
                style={{
                  minWidth: 48,
                  height: 48,
                  borderRadius: 8,
                  background: "var(--celerex-accent, #2563eb)",
                  color: "#fff",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 12,
                  fontWeight: 700,
                  flexShrink: 0,
                }}
              >
                {fmt.ext}
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontSize: 14,
                    fontWeight: 600,
                    color: "var(--celerex-text-primary, #111)",
                    marginBottom: 2,
                  }}
                >
                  {fmt.name}
                </div>
                <div
                  style={{
                    fontSize: 12,
                    color: "var(--celerex-text-secondary, #888)",
                    lineHeight: 1.4,
                  }}
                >
                  {fmt.description}
                </div>
              </div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
