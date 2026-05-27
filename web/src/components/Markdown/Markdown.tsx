import { useMemo } from 'react';
import { renderMarkdown } from '../../lib/markdown';
import styles from './Markdown.module.css';

interface Props {
  source: string;
}

// Renders markdown with the Hearth typography. Styles ported from
// dir-1-hearth.jsx:260-277 into Markdown.module.css.
export function Markdown({ source }: Props) {
  const html = useMemo(() => renderMarkdown(source), [source]);
  return <div className={styles.body} dangerouslySetInnerHTML={{ __html: html }} />;
}
