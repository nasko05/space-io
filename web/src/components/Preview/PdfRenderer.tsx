import { useEffect, useRef, useState } from 'react';
import * as pdfjsLib from 'pdfjs-dist';
import pdfWorkerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import styles from './PdfRenderer.module.css';

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

interface Props {
  data: ArrayBuffer;
}

export default function PdfRenderer({ data }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [pageCount, setPageCount] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const container = containerRef.current;
    if (!container) { return; }
    container.innerHTML = '';

    (async () => {
      try {
        const doc = await pdfjsLib.getDocument({ data: data.slice(0) }).promise;
        if (cancelled) {
          doc.destroy();
          return;
        }
        setPageCount(doc.numPages);
        const scale = Math.min(2, window.devicePixelRatio || 1) * 1.2;
        for (let pageNumber = 1; pageNumber <= doc.numPages; pageNumber += 1) {
          if (cancelled) { break; }
          const page = await doc.getPage(pageNumber);
          const viewport = page.getViewport({ scale });
          const canvas = document.createElement('canvas');
          canvas.className = styles.page;
          canvas.width = viewport.width;
          canvas.height = viewport.height;
          canvas.style.maxWidth = '100%';
          canvas.style.height = 'auto';
          container.appendChild(canvas);
          const context = canvas.getContext('2d');
          if (!context) { continue; }
          await page.render({ canvasContext: context, viewport, canvas }).promise;
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'failed to render PDF');
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [data]);

  return (
    <div className={styles.root}>
      {error && <div className={styles.error}>{error}</div>}
      <div ref={containerRef} className={styles.pages} />
      {pageCount > 0 && (
        <div className={styles.footer}>
          {pageCount} {pageCount === 1 ? 'page' : 'pages'}
        </div>
      )}
    </div>
  );
}
