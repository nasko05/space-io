import { describe, it, expect } from 'vitest';
import { sanitizeHtml } from './sanitizeHtml';

// Adversarial cases for the DOCX→HTML sanitiser. These pin the dangerous
// shapes a hostile document could smuggle through mammoth's output so a future
// refactor of sanitizeHtml can't silently reopen a hole.
describe('sanitizeHtml hardening', () => {
  it('drops a top-level <script>', () => {
    const out = sanitizeHtml('<p>ok</p><script>alert(1)</script>');
    expect(out).toContain('ok');
    expect(out.toLowerCase()).not.toContain('<script');
    expect(out).not.toContain('alert(1)');
  });

  it('drops a <script> nested inside foreign content (SVG)', () => {
    const out = sanitizeHtml('<svg><script>alert(1)</script></svg>');
    expect(out.toLowerCase()).not.toContain('<script');
    expect(out.toLowerCase()).not.toContain('<svg');
    expect(out).not.toContain('alert(1)');
  });

  it('removes <svg> and <math> foreign-content roots entirely', () => {
    expect(sanitizeHtml('<svg width="1"></svg>').toLowerCase()).not.toContain('<svg');
    expect(sanitizeHtml('<math><mi>x</mi></math>').toLowerCase()).not.toContain('<math');
  });

  it('strips on* event-handler attributes', () => {
    const out = sanitizeHtml('<img src="https://example.com/a.png" onerror="alert(1)">');
    expect(out).not.toContain('onerror');
    expect(out).not.toContain('alert(1)');
  });

  it('neutralises javascript: URLs', () => {
    const out = sanitizeHtml('<a href="javascript:alert(1)">x</a>');
    expect(out).not.toContain('javascript:');
  });

  it('neutralises a javascript: URL hidden behind control characters', () => {
    const out = sanitizeHtml('<a href="java\tscript:alert(1)">x</a>');
    expect(out).not.toContain('alert(1)');
  });

  it('removes <iframe> and <object>', () => {
    expect(sanitizeHtml('<iframe src="https://evil"></iframe>').toLowerCase()).not.toContain(
      '<iframe',
    );
    expect(sanitizeHtml('<object data="x"></object>').toLowerCase()).not.toContain('<object');
  });

  it('strips form controls', () => {
    const out = sanitizeHtml('<form action="javascript:alert(1)"><button>go</button></form>');
    expect(out.toLowerCase()).not.toContain('<form');
    expect(out.toLowerCase()).not.toContain('<button');
  });

  it('keeps benign document markup intact', () => {
    const out = sanitizeHtml(
      '<h1>Title</h1><p>A <strong>bold</strong> <a href="https://example.com">link</a>.</p>',
    );
    expect(out).toContain('<h1>Title</h1>');
    expect(out).toContain('<strong>bold</strong>');
    expect(out).toContain('href="https://example.com"');
  });
});
