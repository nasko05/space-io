import { FormEvent, useState } from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { Chevron, Eye } from '../icons/Icon';
import { api, ApiError } from '../../api/client';
import styles from './Registration.module.css';

interface Props {
  /** Render a "back to login" link in the corner when there are already users. */
  showBackToLogin?: boolean;
  onRegistered: (userUuid: string) => void;
  onBackToLogin?: () => void;
}

/**
 * Registration: collects email + passphrase and posts to /api/auth/init, which
 * creates the user's UUID-named folder and records the email→UUID mapping.
 */
export function Registration({ showBackToLogin = false, onRegistered, onBackToLogin }: Props) {
  const [email, setEmail] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [confirm, setConfirm] = useState('');
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function validate(): string | null {
    const trimmed = email.trim();
    if (!trimmed) { return 'Choose an email — it labels the vault on disk.'; }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmed)) {
      return 'That doesn’t look like an email.';
    }
    if (!passphrase) { return 'Choose a passphrase.'; }
    if (passphrase.length < 8) { return 'Make the passphrase at least 8 characters.'; }
    if (passphrase !== confirm) { return 'Those two passphrases don’t match.'; }
    return null;
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const validationError = validate();
    if (validationError) {
      setError(validationError);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await api.init(email.trim(), passphrase);
      onRegistered(result.user_uuid);
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Could not create the space.');
      }
      setBusy(false);
    }
  }

  return (
    <div className={styles.root}>
      <WindowChrome title="SpaceIO · first run" />
      <div className={styles.body}>
        <div className={styles.illustration}>
          <div className={styles.deckled} />
          <div className={styles.page}>
            <div className={styles.pageLabel}>No. I · The first page</div>
            <h2 className={styles.pageHeadline}>
              A diary
              <br />
              <em>without a key</em>
              <br />
              is just paper.
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
              <span>Choose a passphrase you can carry. There's no recovery.</span>
            </div>
          </div>
        </div>

        <div className={styles.formPane}>
          <form className={styles.form} onSubmit={submit}>
            <div className={styles.brand}>S</div>

            <h1 className={styles.welcome}>Make your space.</h1>
            <div className={styles.subhead}>
              Your email labels the vault. We map it to a UUID-named folder on
              disk — the mapping is written to <code>.users.toml</code> and
              survives restarts.
            </div>

            <div className={styles.field}>
              <label className={styles.label} htmlFor="email">
                Email
              </label>
              <div className={styles.identity}>
                <div className={styles.avatar} />
                <input
                  id="email"
                  className={styles.identityInput}
                  type="email"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  placeholder="you@home.lan"
                  spellCheck={false}
                  autoComplete="email"
                  autoFocus
                  disabled={busy}
                />
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
                  onChange={(event) => setPassphrase(event.target.value)}
                  autoComplete="new-password"
                  spellCheck={false}
                  disabled={busy}
                  aria-invalid={error ? 'true' : 'false'}
                />
                <button
                  type="button"
                  className={styles.eyeBtn}
                  onClick={() => setVisible((current) => !current)}
                  tabIndex={-1}
                  aria-label={visible ? 'Hide passphrase' : 'Show passphrase'}
                >
                  <Eye size={15} />
                </button>
              </div>
            </div>

            <div className={styles.field}>
              <label className={styles.label} htmlFor="confirm">
                Confirm passphrase
              </label>
              <div className={`${styles.passWrap} ${error ? styles.passWrapError : ''}`}>
                <input
                  id="confirm"
                  className={styles.passInput}
                  type={visible ? 'text' : 'password'}
                  value={confirm}
                  onChange={(event) => setConfirm(event.target.value)}
                  autoComplete="new-password"
                  spellCheck={false}
                  disabled={busy}
                />
              </div>
              <div className={styles.hintRow}>
                <span className={styles.hintLeft}>
                  <span className={styles.dot} /> stored only as a verifier hash on disk
                </span>
                {error && (
                  <span className={styles.error} role="alert">
                    {error}
                  </span>
                )}
              </div>
            </div>

            <button
              type="submit"
              className={styles.submit}
              disabled={busy || !email || !passphrase || !confirm}
            >
              {busy ? 'Creating…' : 'Open my space'}
              <Chevron size={14} />
            </button>

            {showBackToLogin && onBackToLogin && (
              <button type="button" className={styles.backToLogin} onClick={onBackToLogin}>
                Already have a space? Sign in
              </button>
            )}

            <div className={styles.security}>
              <span>
                <span className={styles.dot} /> end-to-end encrypted
              </span>
              <span>·</span>
              <span>your notes never leave the box</span>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
