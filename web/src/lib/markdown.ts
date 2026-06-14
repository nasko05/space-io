import { Marked, type Tokens, type TokenizerAndRendererExtension } from 'marked';
import { sanitizeHtml } from './sanitizeHtml';

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

interface WikilinkToken extends Tokens.Generic {
  type: 'wikilink';
  text: string;
}

/**
 * `[[Some Note]]` becomes an inert anchor the Markdown component upgrades into
 * navigation. Inline, tried before the stock link tokenizer so `[[…]]` never
 * decays into a reflink.
 */
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
    html(token: Tokens.HTML | Tokens.Tag) {
      return escapeHtml(token.text);
    },
  },
});

/**
 * Render markdown for the Reader/preview: `marked` for full GFM, plus the
 * `[[wikilink]]` extension. Defence in depth — author-typed raw HTML is escaped
 * so it renders as text (only marked's own output is trusted), then the result
 * runs through `sanitizeHtml`.
 */
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

/** Source minus its first H1 line, so the Reader doesn't render the headline
 * twice. Mirrors `extractTitle`'s anchor and also eats trailing blank lines so
 * the body has no leading gap. */
export function stripFirstH1(src: string): string {
  return src.replace(/^# .*(?:\r?\n)*/m, '');
}
