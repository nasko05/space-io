import { describe, it, expect } from 'vitest';
import { renderMarkdown } from './markdown';

describe('markdown XSS safety', () => {
  it('blocks javascript: in links', () => {
    const html = renderMarkdown('[click](javascript:alert(document.cookie))');
    expect(html).toContain('href="#"');
    expect(html).not.toContain('javascript:');
  });

  it('blocks data: URLs', () => {
    const html = renderMarkdown('[x](data:text/html;base64,PHNjcmlwdD4=)');
    expect(html).toContain('href="#"');
    expect(html).not.toContain('data:');
  });

  it('renders a data: URL embedded in broken link syntax as inert text', () => {
    const html = renderMarkdown('[x](data:text/html,<img src=x onerror=alert(1)>)');
    expect(html).not.toMatch(/href="data:/i);
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });

  it('blocks vbscript: URLs', () => {
    const html = renderMarkdown('[x](vbscript:msgbox)');
    expect(html).toContain('href="#"');
  });

  it('blocks file: URLs', () => {
    const html = renderMarkdown('[x](file:///etc/passwd)');
    expect(html).toContain('href="#"');
  });

  it('blocks obfuscated javascript with control chars', () => {
    const html = renderMarkdown('[x](java\tscript:alert(1))');
    expect(html).not.toMatch(/href="[^"]*script:/i);
  });

  it('allows https URLs', () => {
    const html = renderMarkdown('[x](https://safe.example.com)');
    expect(html).toContain('href="https://safe.example.com"');
  });

  it('allows relative URLs', () => {
    const html = renderMarkdown('[x](./page)');
    expect(html).toContain('href="./page"');
  });

  it('allows anchor links', () => {
    const html = renderMarkdown('[x](#section)');
    expect(html).toContain('href="#section"');
  });

  it('escapes HTML entities in text', () => {
    const html = renderMarkdown('<img src=x onerror=alert(1)>');
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });

  it('prevents quotes in a URL from breaking out of the href attribute', () => {
    const html = renderMarkdown('[x](https://a.com/"onmouseover="alert(1))');
    expect(html).not.toContain('"onmouseover=');
    expect(html).not.toMatch(/<a[^>]*\sonmouseover=/i);
  });
});
