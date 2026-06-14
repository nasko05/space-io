import { useEffect, useState } from 'react';
import mammoth from 'mammoth';
import { sanitizeHtml } from '../../lib/sanitizeHtml';
import styles from './DocxRenderer.module.css';

interface Props {
  data: ArrayBuffer;
}

export default function DocxRenderer({ data }: Props) {
  const [html, setHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const result = await mammoth.convertToHtml({ arrayBuffer: data.slice(0) });
        if (!cancelled) { setHtml(sanitizeHtml(result.value)); }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'failed to render DOCX');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [data]);

  if (error) { return <div className={styles.error}>{error}</div>; }
  if (html === null) { return <div className={styles.loading}>Rendering…</div>; }
  return (
    <article className={styles.page}>
      <div className={styles.body} dangerouslySetInnerHTML={{ __html: html }} />
    </article>
  );
}
