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
