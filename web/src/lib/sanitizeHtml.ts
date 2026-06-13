// Second-line HTML sanitiser for the only HTML we render from untrusted input:
// mammoth's DOCX-to-HTML conversion. It strips dangerous elements and `on*`
// handlers and downgrades unsafe URL schemes to `#`. Not a DOMPurify
// replacement for adversarial settings.

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
  // SVG/MathML foreign-content roots and template/noscript are classic
  // mutation-XSS levers (a safe-looking tree re-parses into something
  // executable); mammoth never emits them, so forbid them outright.
  'svg',
  'math',
  'template',
  'noscript',
  // Form controls carry submission behaviour and aren't document content.
  // `input` is handled separately so GFM task-list checkboxes survive.
  'form',
  'button',
  'textarea',
  'select',
  'option',
]);

const URL_ATTRS = new Set(['href', 'src', 'xlink:href', 'action', 'formaction']);

// The inert GFM task-list checkbox is the only `<input>` allowed through, pared
// down to these attributes so nothing like `onfocus=` or `formaction=` rides in.
const CHECKBOX_KEEP_ATTRS = new Set(['type', 'checked', 'disabled']);

function isAllowedCheckbox(el: Element): boolean {
  return el.tagName.toLowerCase() === 'input' && el.getAttribute('type') === 'checkbox';
}

function isSafeUrl(raw: string): boolean {
  const trimmed = raw.trim();
  if (trimmed === '') return true;
  // Reject control chars first: some browsers fold `java\tscript:` back into a
  // live scheme.
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
  const children = Array.from(node.children);
  for (const child of children) {
    const tag = child.tagName.toLowerCase();
    if (FORBIDDEN_TAGS.has(tag)) {
      child.remove();
      continue;
    }
    if (tag === 'input') {
      if (!isAllowedCheckbox(child)) {
        child.remove();
        continue;
      }
      for (const attr of Array.from(child.attributes)) {
        if (!CHECKBOX_KEEP_ATTRS.has(attr.name.toLowerCase())) {
          child.removeAttribute(attr.name);
        }
      }
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
