import { describe, it, expect } from 'vitest';
import { renderMarkdown, extractTitle, stripFirstH1 } from './markdown';

describe('renderMarkdown', () => {
  it('renders headings', () => {
    expect(renderMarkdown('# Hello')).toBe('<h1>Hello</h1>');
    expect(renderMarkdown('## Sub')).toBe('<h2>Sub</h2>');
    expect(renderMarkdown('### Third')).toBe('<h3>Third</h3>');
  });

  it('renders inline bold and italic', () => {
    expect(renderMarkdown('**bold**')).toBe('<p><strong>bold</strong></p>');
    expect(renderMarkdown('*em*')).toBe('<p><em>em</em></p>');
  });

  it('renders inline code', () => {
    expect(renderMarkdown('use `fn()`')).toBe('<p>use <code>fn()</code></p>');
  });

  it('renders wikilinks as anchor tags', () => {
    const html = renderMarkdown('see [[My Note]]');
    expect(html).toContain('class="wikilink"');
    expect(html).toContain('My Note');
  });

  it('renders markdown links with safe href', () => {
    const html = renderMarkdown('[click](https://example.com)');
    expect(html).toContain('href="https://example.com"');
    expect(html).toContain('>click</a>');
  });

  it('blocks javascript: URLs', () => {
    const html = renderMarkdown('[x](javascript:alert(1))');
    expect(html).toContain('href="#"');
  });

  it('blocks data: URLs', () => {
    const html = renderMarkdown('[x](data:text/html,<script>)');
    expect(html).toContain('href="#"');
  });

  it('renders unordered lists', () => {
    const html = renderMarkdown('- one\n- two');
    expect(html).toContain('<ul>');
    expect(html).toContain('<li>one</li>');
    expect(html).toContain('<li>two</li>');
    expect(html).toContain('</ul>');
  });

  it('renders ordered lists', () => {
    const html = renderMarkdown('1. first\n2. second');
    expect(html).toContain('<ol>');
    expect(html).toContain('<li>first</li>');
  });

  it('renders blockquotes', () => {
    const html = renderMarkdown('> quoted');
    expect(html).toContain('<blockquote>');
    expect(html).toContain('quoted');
  });

  it('renders horizontal rules', () => {
    expect(renderMarkdown('---')).toBe('<hr/>');
  });

  it('returns empty for empty input', () => {
    expect(renderMarkdown('')).toBe('');
  });

  it('escapes HTML in plain text', () => {
    const html = renderMarkdown('<script>alert(1)</script>');
    expect(html).not.toContain('<script>');
    expect(html).toContain('&lt;script&gt;');
  });

  it('closes open lists before switching to paragraphs', () => {
    const html = renderMarkdown('- item\n\nparagraph');
    expect(html).toContain('</ul>');
    expect(html).toContain('<p>paragraph</p>');
  });
});

describe('extractTitle', () => {
  it('returns the first H1 content', () => {
    expect(extractTitle('# Hello World\n\nbody')).toBe('Hello World');
  });

  it('returns null when no H1 exists', () => {
    expect(extractTitle('## Not H1\n\nbody')).toBeNull();
  });

  it('trims whitespace from the title', () => {
    expect(extractTitle('#   Spaced   ')).toBe('Spaced');
  });
});

describe('stripFirstH1', () => {
  it('removes the first H1 and trailing blank lines', () => {
    const input = '# Title\n\nBody here';
    expect(stripFirstH1(input)).toBe('Body here');
  });

  it('leaves content intact if no H1', () => {
    const input = 'Just body text';
    expect(stripFirstH1(input)).toBe('Just body text');
  });

  it('only strips the first H1', () => {
    const input = '# First\n\n# Second\n\nBody';
    const result = stripFirstH1(input);
    expect(result).toContain('# Second');
    expect(result).not.toContain('# First');
  });
});
