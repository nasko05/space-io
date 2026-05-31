// Minimal HTML sanitiser for trusted-ish renderer outputs (e.g. mammoth.js
// turning DOCX into HTML, where a hostile document could embed an
// onerror handler or a `javascript:` URL).
//
// Goals:
//   - strip <script>, <iframe>, <object>, <embed>, <link>, <meta>, <style>
//     (style sheets can ship javascript: URLs in some old browsers and
//     leak resources cross-origin via @import).
//   - strip every `on*` event-handler attribute on every element.
//   - downgrade href/src/xlink:href schemes to `#` unless they're safe
//     (http(s), mailto, anchor, or relative).
//
// This is not a substitute for DOMPurify in adversarial settings — we
// rely on it as a second line of defence behind the explicit fact that
// the only HTML we ever feed it is mammoth's DOCX-to-HTML conversion.

const FORBIDDEN_TAGS = new Set([
  'script',
  'iframe',
  'object',
  'embed',
  'link',
  'meta',
  'style',
  'base',
  'frame',
  'frameset',
  // Foreign-content roots (SVG/MathML) get their own HTML parsing rules, which
  // is the classic mutation-XSS lever: a tree that looks safe after one parse
  // can re-parse into something executable. mammoth never emits these from a
  // DOCX, so forbidding them outright closes the hole with no legitimate loss.
  'svg',
  'math',
  // <template>/<noscript> hold inert content that the browser re-parses in a
  // different mode when the result is re-inserted — another mXSS vector.
  'template',
  'noscript',
  // Form controls can carry formaction/submission behaviour and aren't part of
  // a rendered document.
  'form',
  'input',
  'button',
  'textarea',
  'select',
  'option',
]);

const URL_ATTRS = new Set(['href', 'src', 'xlink:href', 'action', 'formaction']);

function isSafeUrl(raw: string): boolean {
  const trimmed = raw.trim();
  if (trimmed === '') return true;
  // Strip whitespace-and-control noise hidden inside the scheme name —
  // some browsers fold `java\tscript:` back to `javascript:`.
  if (/[\x00-\x1f\x7f]/.test(trimmed)) return false;
  if (/^\s*(javascript|data|vbscript|file)\s*:/i.test(trimmed)) return false;
  return true;
}

export function sanitizeHtml(html: string): string {
  if (!html) return '';
  const doc = new DOMParser().parseFromString(html, 'text/html');
  walk(doc.body);
  return doc.body.innerHTML;
}

function walk(node: Element) {
  // Snapshot children first — we mutate the tree as we go.
  const kids = Array.from(node.children);
  for (const child of kids) {
    const tag = child.tagName.toLowerCase();
    if (FORBIDDEN_TAGS.has(tag)) {
      child.remove();
      continue;
    }
    // Drop `on*` handlers and unsafe URL attributes.
    for (const attr of Array.from(child.attributes)) {
      const name = attr.name.toLowerCase();
      if (name.startsWith('on')) {
        child.removeAttribute(attr.name);
        continue;
      }
      if (URL_ATTRS.has(name) && !isSafeUrl(attr.value)) {
        child.setAttribute(attr.name, '#');
      }
    }
    walk(child);
  }
}
