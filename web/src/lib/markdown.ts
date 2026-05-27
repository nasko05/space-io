// Tiny markdown renderer — ported from diary-data.js:119-171.
// Only what we need for the Hearth prototype; replace with a real parser later.

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function inline(s: string): string {
  return escapeHtml(s)
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\[\[([^\]]+)\]\]/g, '<a class="wikilink" href="#">$1</a>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
}

export function renderMarkdown(src: string): string {
  if (!src) return '';
  const lines = src.split('\n');
  const out: string[] = [];
  let listKind: 'ul' | 'ol' | null = null;
  let inQuote = false;

  const close = () => {
    if (listKind) {
      out.push(`</${listKind}>`);
      listKind = null;
    }
    if (inQuote) {
      out.push('</blockquote>');
      inQuote = false;
    }
  };

  for (const raw of lines) {
    const line = raw.replace(/\s+$/, '');
    if (/^---\s*$/.test(line)) {
      close();
      out.push('<hr/>');
      continue;
    }
    if (/^# /.test(line)) {
      close();
      out.push(`<h1>${inline(line.slice(2))}</h1>`);
      continue;
    }
    if (/^## /.test(line)) {
      close();
      out.push(`<h2>${inline(line.slice(3))}</h2>`);
      continue;
    }
    if (/^### /.test(line)) {
      close();
      out.push(`<h3>${inline(line.slice(4))}</h3>`);
      continue;
    }
    if (/^> /.test(line)) {
      if (listKind) {
        out.push(`</${listKind}>`);
        listKind = null;
      }
      if (!inQuote) {
        out.push('<blockquote>');
        inQuote = true;
      }
      out.push(`<p>${inline(line.slice(2))}</p>`);
      continue;
    }
    if (/^- /.test(line)) {
      if (inQuote) {
        out.push('</blockquote>');
        inQuote = false;
      }
      if (listKind !== 'ul') {
        if (listKind) out.push(`</${listKind}>`);
        out.push('<ul>');
        listKind = 'ul';
      }
      out.push(`<li>${inline(line.slice(2))}</li>`);
      continue;
    }
    if (/^\d+\. /.test(line)) {
      if (inQuote) {
        out.push('</blockquote>');
        inQuote = false;
      }
      if (listKind !== 'ol') {
        if (listKind) out.push(`</${listKind}>`);
        out.push('<ol>');
        listKind = 'ol';
      }
      out.push(`<li>${inline(line.replace(/^\d+\. /, ''))}</li>`);
      continue;
    }
    if (line === '') {
      close();
      continue;
    }
    close();
    out.push(`<p>${inline(line)}</p>`);
  }
  close();
  return out.join('');
}

/** Pull the first `# Heading` from a markdown source. */
export function extractTitle(src: string): string | null {
  const match = src.match(/^# (.+)$/m);
  return match ? match[1].trim() : null;
}

/** Pull the source minus the leading H1 line (so we don't render it twice). */
export function stripFirstH1(src: string): string {
  return src.replace(/^# .*\n+/, '');
}
