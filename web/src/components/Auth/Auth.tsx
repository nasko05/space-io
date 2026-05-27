import { FormEvent, useState } from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { Chevron, Eye } from '../icons/Icon';
import { api, ApiError } from '../../api/client';
import styles from './Auth.module.css';

interface Props {
  owner: string;
  onUnlocked: () => void;
}

// Ported from dir-1-hearth.jsx:515-666. Replaces the decorative bullet display
// with a real <input type="password">; passkey block (originally 636-653) is
// deferred to Phase 2.
export function Auth({ owner, onUnlocked }: Props) {
  const [passphrase, setPassphrase] = useState('');
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!passphrase) return;
    setBusy(true);
    setError(null);
    try {
      await api.unlock(passphrase);
      onUnlocked();
    } catch (err) {
      if (err instanceof ApiError && err.code === 'wrong_passphrase') {
        setError('Wrong passphrase');
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Could not unlock');
      }
      setBusy(false);
    }
  }

  return (
    <div className={styles.root}>
      <WindowChrome title="SpaceIO · locked" />
      <div className={styles.body}>
        <div className={styles.illustration}>
          <div className={styles.deckled} />
          <div className={styles.page}>
            <div className={styles.pageLabel}>No. CCXLVII · A page from</div>
            <h2 className={styles.pageHeadline}>
              The diary that
              <br />
              <em>only you</em>
              <br />
              can open.
            </h2>
            <div className={styles.pageFoot}>
              <svg width={44} height={22} viewBox="0 0 44 22" aria-hidden>
                <path
                  d="M2 11 q 8 -10 14 0 t 14 0 t 12 0"
                  fill="none"
                  stroke="var(--accent)"
                  strokeWidth={1.2}
                />
              </svg>
              <span>1,247 entries · 6 years · self-hosted on home server</span>
            </div>
          </div>
        </div>

        <div className={styles.formPane}>
          <form className={styles.form} onSubmit={submit}>
            <div className={styles.brand}>S</div>

            <h1 className={styles.welcome}>Welcome back.</h1>
            <div className={styles.subhead}>Last opened yesterday at 22:51.</div>

            <div className={styles.field}>
              <label className={styles.label}>You</label>
              <div className={styles.identity}>
                <div className={styles.avatar} />
                <span className={styles.identityName}>{owner}</span>
                <span className={styles.identityAlt}>not you?</span>
              </div>
            </div>

            <div className={styles.field}>
              <label className={styles.label} htmlFor="passphrase">
                Passphrase
              </label>
              <div className={`${styles.passWrap} ${error ? styles.passWrapError : ''}`}>
                <input
                  id="passphrase"
                  className={styles.passInput}
                  type={visible ? 'text' : 'password'}
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                  autoFocus
                  autoComplete="current-password"
                  spellCheck={false}
                  disabled={busy}
                  aria-invalid={error ? 'true' : 'false'}
                />
                <button
                  type="button"
                  className={styles.eyeBtn}
                  onClick={() => setVisible((v) => !v)}
                  tabIndex={-1}
                  aria-label={visible ? 'Hide passphrase' : 'Show passphrase'}
                >
                  <Eye size={15} />
                </button>
              </div>
              <div className={styles.hintRow}>
                <span className={styles.hintLeft}>
                  <span className={styles.dot} /> on this device only
                </span>
                {error ? (
                  <span className={styles.error} role="alert">
                    {error}
                  </span>
                ) : (
                  <a className={styles.recovery}>recovery phrase →</a>
                )}
              </div>
            </div>

            <button type="submit" className={styles.submit} disabled={busy || !passphrase}>
              {busy ? 'Opening…' : 'Open my space'}
              <Chevron size={14} />
            </button>

            <div className={styles.security}>
              <span>
                <span className={styles.dot} /> end-to-end encrypted
              </span>
              <span>·</span>
              <span>home.lan reachable</span>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
