/**
 * Minimal markdown renderer.
 *
 * Covers ~80% of common cases without any third-party dependency:
 *   - ATX headings: # H1, ## H2, ### H3
 *   - Unordered lists: lines starting with "- " or "* "
 *   - Ordered lists: lines starting with "1. " (any digits)
 *   - Fenced code blocks: ```...```
 *   - Inline: **bold**, *italic*, _italic_, `code`, [text](url)
 *   - Everything else: plain paragraph
 *
 * Out of scope (intentional): tables, images, blockquotes, setext headings,
 * nested lists, HTML passthrough.
 *
 * Returns plain React elements — never uses dangerouslySetInnerHTML, so React's
 * normal escaping handles the content safely.
 */

import { Fragment, type JSX, type ReactNode } from "react";

const HEADING_STYLES: Record<1 | 2 | 3, React.CSSProperties> = {
  1: {
    margin: "16px 0 8px 0",
    fontSize: 22,
    fontWeight: 700,
    lineHeight: 1.25,
    color: "var(--celerex-text-primary, inherit)",
  },
  2: {
    margin: "14px 0 6px 0",
    fontSize: 18,
    fontWeight: 700,
    lineHeight: 1.3,
    color: "var(--celerex-text-primary, inherit)",
  },
  3: {
    margin: "12px 0 4px 0",
    fontSize: 15,
    fontWeight: 600,
    lineHeight: 1.35,
    color: "var(--celerex-text-primary, inherit)",
  },
};

const PARAGRAPH_STYLE: React.CSSProperties = {
  margin: "6px 0",
  fontSize: 14,
  lineHeight: 1.55,
};

const LIST_STYLE: React.CSSProperties = {
  margin: "6px 0",
  paddingLeft: 24,
  fontSize: 14,
  lineHeight: 1.55,
};

const LIST_ITEM_STYLE: React.CSSProperties = {
  margin: "2px 0",
};

const CODE_INLINE_STYLE: React.CSSProperties = {
  fontFamily:
    "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
  fontSize: "0.92em",
  padding: "1px 5px",
  borderRadius: 4,
  background: "var(--celerex-bg-secondary, rgba(127, 127, 127, 0.12))",
  border: "1px solid var(--celerex-border, rgba(127, 127, 127, 0.18))",
};

const CODE_BLOCK_STYLE: React.CSSProperties = {
  margin: "8px 0",
  padding: 10,
  borderRadius: 6,
  background: "var(--celerex-bg-secondary, rgba(127, 127, 127, 0.08))",
  border: "1px solid var(--celerex-border, rgba(127, 127, 127, 0.18))",
  overflowX: "auto",
  fontSize: 12.5,
  lineHeight: 1.5,
  fontFamily:
    "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
  whiteSpace: "pre",
};

const LINK_STYLE: React.CSSProperties = {
  color: "var(--celerex-accent, #2563eb)",
  textDecoration: "underline",
};

type Block =
  | { kind: "heading"; level: 1 | 2 | 3; text: string }
  | { kind: "ul"; items: string[] }
  | { kind: "ol"; items: string[] }
  | { kind: "code"; text: string; lang?: string }
  | { kind: "paragraph"; text: string }
  | { kind: "blank" };

/**
 * Parse markdown source into a flat list of blocks. Single pass, line-based.
 * Consecutive list items are grouped into a single ul/ol block.
 */
function parseBlocks(source: string): Block[] {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];

  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";

    // Fenced code block: ``` (optionally with language)
    const fenceMatch = line.match(/^```(\w*)\s*$/);
    if (fenceMatch) {
      const lang = fenceMatch[1] || undefined;
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i] ?? "")) {
        codeLines.push(lines[i] ?? "");
        i++;
      }
      // Skip the closing fence (if present)
      if (i < lines.length) i++;
      blocks.push({ kind: "code", text: codeLines.join("\n"), lang });
      continue;
    }

    // Headings
    const h1 = /^# (.*)$/.exec(line);
    const h2 = /^## (.*)$/.exec(line);
    const h3 = /^### (.*)$/.exec(line);
    if (h1) {
      blocks.push({ kind: "heading", level: 1, text: h1[1] ?? "" });
      i++;
      continue;
    }
    if (h2) {
      blocks.push({ kind: "heading", level: 2, text: h2[1] ?? "" });
      i++;
      continue;
    }
    if (h3) {
      blocks.push({ kind: "heading", level: 3, text: h3[1] ?? "" });
      i++;
      continue;
    }

    // Unordered list
    if (/^[-*] +.+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*] +.+/.test(lines[i] ?? "")) {
        items.push((lines[i] ?? "").replace(/^[-*] +/, ""));
        i++;
      }
      blocks.push({ kind: "ul", items });
      continue;
    }

    // Ordered list
    if (/^\d+\. +.+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\. +.+/.test(lines[i] ?? "")) {
        items.push((lines[i] ?? "").replace(/^\d+\. +/, ""));
        i++;
      }
      blocks.push({ kind: "ol", items });
      continue;
    }

    // Blank line
    if (line.trim() === "") {
      blocks.push({ kind: "blank" });
      i++;
      continue;
    }

    // Paragraph: gather consecutive non-empty, non-special lines
    const paraLines: string[] = [line];
    i++;
    while (i < lines.length) {
      const next = lines[i] ?? "";
      if (
        next.trim() === "" ||
        /^#{1,3} /.test(next) ||
        /^[-*] +/.test(next) ||
        /^\d+\. +/.test(next) ||
        /^```/.test(next)
      ) {
        break;
      }
      paraLines.push(next);
      i++;
    }
    blocks.push({ kind: "paragraph", text: paraLines.join(" ") });
  }

  return blocks;
}

/**
 * Inline tokenizer. Order matters: code spans first (their content should not
 * be re-parsed), then links, then bold (uses `**`, longer marker before `*`),
 * then italic.
 *
 * Returns React nodes; raw text segments are emitted as plain strings so React
 * escapes them automatically.
 */
function renderInline(text: string, keyBase: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let remaining = text;
  let counter = 0;

  // Single regex with alternation, evaluated left-to-right via exec loop.
  // Capturing groups (in order):
  //   1. inline code:    `code`
  //   2. bold:           **bold**
  //   3. italic (star):  *italic*
  //   4. italic (under): _italic_
  //   5/6. link text/url: [text](url)
  const pattern =
    /`([^`\n]+)`|\*\*([^*\n]+)\*\*|\*([^*\n]+)\*|_([^_\n]+)_|\[([^\]\n]+)\]\(([^)\s]+)\)/;

  while (remaining.length > 0) {
    const m = pattern.exec(remaining);
    if (!m) {
      nodes.push(remaining);
      break;
    }
    const matchStart = m.index;
    if (matchStart > 0) {
      nodes.push(remaining.slice(0, matchStart));
    }

    const key = `${keyBase}-i${counter++}`;
    if (m[1] !== undefined) {
      nodes.push(
        <code key={key} style={CODE_INLINE_STYLE}>
          {m[1]}
        </code>,
      );
    } else if (m[2] !== undefined) {
      nodes.push(<strong key={key}>{renderInline(m[2], key)}</strong>);
    } else if (m[3] !== undefined) {
      nodes.push(<em key={key}>{renderInline(m[3], key)}</em>);
    } else if (m[4] !== undefined) {
      nodes.push(<em key={key}>{renderInline(m[4], key)}</em>);
    } else if (m[5] !== undefined && m[6] !== undefined) {
      nodes.push(
        <a
          key={key}
          href={m[6]}
          target="_blank"
          rel="noreferrer"
          style={LINK_STYLE}
        >
          {renderInline(m[5], key)}
        </a>,
      );
    }

    remaining = remaining.slice(matchStart + m[0].length);
  }

  return nodes;
}

function renderBlock(block: Block, idx: number): ReactNode {
  const key = `b${idx}`;
  switch (block.kind) {
    case "heading": {
      const inline = renderInline(block.text, key);
      if (block.level === 1) {
        return (
          <h1 key={key} style={HEADING_STYLES[1]}>
            {inline}
          </h1>
        );
      }
      if (block.level === 2) {
        return (
          <h2 key={key} style={HEADING_STYLES[2]}>
            {inline}
          </h2>
        );
      }
      return (
        <h3 key={key} style={HEADING_STYLES[3]}>
          {inline}
        </h3>
      );
    }
    case "ul":
      return (
        <ul key={key} style={LIST_STYLE}>
          {block.items.map((item, j) => (
            <li key={`${key}-${j}`} style={LIST_ITEM_STYLE}>
              {renderInline(item, `${key}-${j}`)}
            </li>
          ))}
        </ul>
      );
    case "ol":
      return (
        <ol key={key} style={LIST_STYLE}>
          {block.items.map((item, j) => (
            <li key={`${key}-${j}`} style={LIST_ITEM_STYLE}>
              {renderInline(item, `${key}-${j}`)}
            </li>
          ))}
        </ol>
      );
    case "code":
      return (
        <pre key={key} style={CODE_BLOCK_STYLE} data-lang={block.lang ?? ""}>
          <code>{block.text}</code>
        </pre>
      );
    case "paragraph":
      return (
        <p key={key} style={PARAGRAPH_STYLE}>
          {renderInline(block.text, key)}
        </p>
      );
    case "blank":
      return null;
  }
}

export function Markdown({ source }: { source: string }): JSX.Element {
  const blocks = parseBlocks(source ?? "");
  return (
    <div data-testid="markdown-rendered" style={{ fontFamily: "inherit" }}>
      {blocks.map((b, idx) => (
        <Fragment key={idx}>{renderBlock(b, idx)}</Fragment>
      ))}
    </div>
  );
}
