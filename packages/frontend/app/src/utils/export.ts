/**
 * Utility to export BlockSuite doc content as Markdown and trigger a browser download.
 */

function blockToMarkdown(block: any): string | null {
  const flavour: string = block.flavour;
  const yBlock = block.yBlock;

  if (flavour === 'affine:divider') {
    return '---';
  }

  const text = yBlock?.get('prop:text')?.toString() ?? '';

  if (flavour === 'affine:paragraph') {
    const type: string = yBlock?.get('prop:type') ?? 'text';
    switch (type) {
      case 'h1':
        return `# ${text}`;
      case 'h2':
        return `## ${text}`;
      case 'h3':
        return `### ${text}`;
      case 'h4':
        return `#### ${text}`;
      case 'quote':
        return `> ${text}`;
      default:
        return text;
    }
  }

  if (flavour === 'affine:list') {
    const type: string = yBlock?.get('prop:type') ?? 'bulleted';
    if (type === 'numbered') {
      return `1. ${text}`;
    }
    return `- ${text}`;
  }

  if (flavour === 'affine:code') {
    return `\`\`\`\n${text}\n\`\`\``;
  }

  return null;
}

export function exportDocAsMarkdown(store: any): void {
  const root = store.root;
  if (!root) {
    return;
  }

  const title = root.yBlock?.get('prop:title')?.toString() ?? 'Untitled';
  const lines: string[] = [`# ${title}`];

  const children: any[] = root.children ?? [];
  for (const child of children) {
    const md = blockToMarkdown(child);
    if (md !== null) {
      lines.push(md);
    }
  }

  const content = lines.join('\n\n');
  const filename = `${title.replace(/[^a-zA-Z0-9_-]/g, '_')}.md`;
  downloadAsFile(content, filename);
}

function blockToHtml(block: any): string | null {
  const flavour: string = block.flavour;
  const yBlock = block.yBlock;

  if (flavour === 'affine:divider') {
    return '<hr>';
  }

  const text = yBlock?.get('prop:text')?.toString() ?? '';
  const escaped = text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  if (flavour === 'affine:paragraph') {
    const type: string = yBlock?.get('prop:type') ?? 'text';
    switch (type) {
      case 'h1':
        return `<h1>${escaped}</h1>`;
      case 'h2':
        return `<h2>${escaped}</h2>`;
      case 'h3':
        return `<h3>${escaped}</h3>`;
      case 'h4':
        return `<h4>${escaped}</h4>`;
      case 'quote':
        return `<blockquote>${escaped}</blockquote>`;
      default:
        return `<p>${escaped}</p>`;
    }
  }

  if (flavour === 'affine:list') {
    const type: string = yBlock?.get('prop:type') ?? 'bulleted';
    if (type === 'numbered') {
      return `<ol><li>${escaped}</li></ol>`;
    }
    return `<ul><li>${escaped}</li></ul>`;
  }

  if (flavour === 'affine:code') {
    return `<pre><code>${escaped}</code></pre>`;
  }

  return null;
}

export function printDocument(store: any): void {
  const root = store.root;
  if (!root) {
    return;
  }

  const title = root.yBlock?.get('prop:title')?.toString() ?? 'Untitled';
  const bodyParts: string[] = [];

  const children: any[] = root.children ?? [];
  for (const child of children) {
    const html = blockToHtml(child);
    if (html !== null) {
      bodyParts.push(html);
    }
  }

  const escapedTitle = title
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  const htmlDoc = `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>${escapedTitle}</title>
<style>
  body {
    max-width: 700px;
    margin: 40px auto;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 14px;
    line-height: 1.6;
    color: #222;
  }
  h1 { font-size: 24px; margin: 24px 0 12px; }
  h2 { font-size: 20px; margin: 20px 0 10px; }
  h3 { font-size: 17px; margin: 18px 0 8px; }
  h4 { font-size: 15px; margin: 16px 0 6px; }
  p { margin: 8px 0; }
  blockquote {
    margin: 12px 0;
    padding: 8px 16px;
    border-left: 3px solid #ccc;
    color: #555;
  }
  pre {
    background: #f5f5f5;
    padding: 12px;
    border-radius: 4px;
    overflow-x: auto;
    font-size: 13px;
  }
  code { font-family: "SFMono-Regular", Consolas, monospace; }
  hr { border: none; border-top: 1px solid #ddd; margin: 20px 0; }
  ul, ol { margin: 8px 0; padding-left: 24px; }
  @media print {
    body { margin: 0 auto; }
  }
</style>
</head>
<body>
<h1>${escapedTitle}</h1>
${bodyParts.join('\n')}
</body>
</html>`;

  const iframe = document.createElement('iframe');
  iframe.style.position = 'fixed';
  iframe.style.top = '-10000px';
  iframe.style.left = '-10000px';
  iframe.style.width = '0';
  iframe.style.height = '0';
  iframe.style.border = 'none';
  document.body.appendChild(iframe);

  const iframeDoc = iframe.contentDocument || iframe.contentWindow?.document;
  if (!iframeDoc) {
    document.body.removeChild(iframe);
    return;
  }

  iframeDoc.open();
  iframeDoc.write(htmlDoc);
  iframeDoc.close();

  // Wait for content to render before printing
  iframe.onload = () => {
    iframe.contentWindow?.print();
    // Remove iframe after a short delay to allow print dialog to open
    setTimeout(() => {
      document.body.removeChild(iframe);
    }, 1000);
  };

  // Fallback: if onload doesn't fire (content already loaded via write)
  setTimeout(() => {
    try {
      iframe.contentWindow?.print();
    } catch (_) {
      // already printing or iframe removed
    }
    setTimeout(() => {
      if (iframe.parentNode) {
        document.body.removeChild(iframe);
      }
    }, 1000);
  }, 250);
}

export function downloadAsFile(content: string, filename: string): void {
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);

  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.style.display = 'none';
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);

  URL.revokeObjectURL(url);
}
