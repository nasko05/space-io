import { MouseEvent, useMemo } from 'react';
import { renderMarkdown } from '../../lib/markdown';
import styles from './Markdown.module.css';

interface Props {
  source: string;
  onWikilinkClick?: (title: string) => void;
}

/**
 * Renders markdown with the SpaceIO typography. Wikilink clicks walk up a few
 * ancestors so a click landing on a child of the `.wikilink` anchor still
 * resolves to the link.
 */
export function Markdown({ source, onWikilinkClick }: Props) {
  const html = useMemo(() => renderMarkdown(source), [source]);

  function onClick(event: MouseEvent<HTMLDivElement>) {
    if (!onWikilinkClick) { return; }
    const target = event.target as HTMLElement | null;
    if (!target) { return; }
    let node: HTMLElement | null = target;
    for (let i = 0; i < 4 && node; i += 1) {
      if (node.classList?.contains('wikilink')) {
        event.preventDefault();
        onWikilinkClick(node.textContent?.trim() ?? '');
        return;
      }
      node = node.parentElement;
    }
  }

  return (
    <div
      className={styles.body}
      onClick={onClick}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
