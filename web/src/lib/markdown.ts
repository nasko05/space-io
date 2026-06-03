// Markdown rendering for the Reader/preview. Backed by `marked` for full
// GitHub-Flavoured Markdown (tables, task lists, strikethrough, fenced code,
// autolinks) with two Hearth-specific adaptations layered on top:
//
//   1. `[[wikilink]]` support — a custom inline extension emits the exact
//      `<a class="wikilink" href="#">Title</a>` markup that Markdown.tsx's
//      click handler keys off of (it matches `.wikilink` and reads
//      `textContent`), so navigation keeps working unchanged.
//   2. Defence in depth — raw HTML in the source is escaped to entities (so a
//      pasted `<script>` renders as text, never executes), and the final
//      string is run through `sanitizeHtml`, which strips dangerous elements
//      and downgrades unsafe URL schemes (`javascript:`, `data:`, …) to `#`.
//
// `extractTitle` / `stripFirstH1` stay regex-based over the *raw* source — the
// Reader uses them to pull the headline before rendering the body, and they
// must not depend on the HTML pipeline.

import { Marked, type Tokens, type TokenizerAndRendererExtension } from 'marked';
import { sanitizeHtml } from './sanitizeHtml';

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

interface WikilinkToken extends Tokens.Generic {
  type: 'wikilink';
  text: string;
}

// `[[Some Note]]` → an inert anchor the Markdown component upgrades into a
// navigation action. Inline level, tried before the stock link tokenizer so a
// bare `[[…]]` never decays into a reflink.
const wikilink: TokenizerAndRendererExtension = {
  name: 'wikilink',
  level: 'inline',
  start(src: string) {
    const i = src.indexOf('[[');
    return i < 0 ? undefined : i;
  },
  tokenizer(src: string) {
    const m = /^\[\[([^\]\n]+)\]\]/.exec(src);
    if (!m) return undefined;
    return { type: 'wikilink', raw: m[0], text: m[1].trim() } satisfies WikilinkToken;
  },
  renderer(token) {
    return `<a class="wikilink" href="#">${escapeHtml((token as WikilinkToken).text)}</a>`;
  },
};

const md = new Marked({ gfm: true, breaks: true });
md.use({
  extensions: [wikilink],
  renderer: {
    // Escape raw inline/block HTML instead of passing it through. The only
    // trusted HTML in the document is what `marked` itself emits from markdown;
    // anything the author literally typed as a tag is shown verbatim as text.
    html(token: Tokens.HTML | Tokens.Tag) {
      return escapeHtml(token.text);
    },
  },
});

export function renderMarkdown(src: string): string {
  if (!src) return '';
  const html = md.parse(src, { async: false });
  return sanitizeHtml(html).trim();
}

/** Pull the first `# Heading` from a markdown source. */
export function extractTitle(src: string): string | null {
  const match = src.match(/^# (.+)$/m);
  return match ? match[1].trim() : null;
}

/** Source minus the first H1 line (so the Reader doesn't render it twice —
 * once as the styled headline, once inside the markdown body).
 *
 * Mirrors `extractTitle` exactly so the two never disagree: `/m` so the
 * anchor matches the first H1 on any line (after a frontmatter block, a
 * leading blank line, CRLF endings, etc.), and the trailing
 * `(?:\r?\n)*` eats any trailing blank lines so the body doesn't start
 * with a stray gap. */
export function stripFirstH1(src: string): string {
  return src.replace(/^# .*(?:\r?\n)*/m, '');
}
