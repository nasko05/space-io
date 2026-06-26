import { FormEvent, useEffect, useState } from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { Chevron, Eye } from '../icons/Icon';
import { api, ApiError } from '../../api/client';
import { isPasskeySupported, unlockWithPasskey } from '../../lib/passkey';
import styles from './Auth.module.css';

/** Full URL of the co-hosted cloud drive, for the "back to drive" link. */
const DRIVE_URL = (import.meta.env.VITE_DRIVE_URL as string | undefined) ?? '';

interface Props {
  /** Whether a "Create another account" link should appear at the bottom. */
  showRegisterLink?: boolean;
  onUnlocked: () => void;
  onRegister?: () => void;
}

/**
 * Multi-tenant login by email + passphrase. Unknown emails surface as "wrong
 * passphrase" so the form doesn't leak which addresses are registered.
 */
export function Auth({ showRegisterLink = false, onUnlocked, onRegister }: Props) {
  const [email, setEmail] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [passkeyBusy, setPasskeyBusy] = useState(false);
  const [driveEmail, setDriveEmail] = useState<string | null>(null);

  // If the visitor arrived with a cloud-drive SSO cookie, greet them by name
  // and prefill the email so they only need to unlock their space.
  useEffect(() => {
    let cancelled = false;
    api
      .sso()
      .then((sso) => {
        if (cancelled || !sso.signed_in || !sso.email) { return; }
        setDriveEmail(sso.email);
        setEmail((current) => current || sso.email!);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!email.trim() || !passphrase) { return; }
    await runUnlock(email.trim(), passphrase);
  }

  async function runUnlock(emailAddress: string, secret: string) {
    setBusy(true);
    setError(null);
    try {
      await api.unlock(emailAddress, secret);
      onUnlocked();
    } catch (err) {
      if (err instanceof ApiError && err.code === 'wrong_passphrase') {
        setError('That email and passphrase don’t open anything.');
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Could not unlock');
      }
      setBusy(false);
    }
  }

  async function tryPasskey() {
    if (passkeyBusy) { return; }
    if (!email.trim()) {
      setError('Type your email first — that’s how we find the right passkey.');
      return;
    }
    setPasskeyBusy(true);
    setError(null);
    try {
      const info = await api.passkeyInfo(email.trim());
      if (!info) {
        setError('No passkey registered for that email.');
        return;
      }
      const recovered = await unlockWithPasskey({
        credentialIdB64: info.credential_id_b64,
        prfSaltB64: info.prf_salt_b64,
        wrappedPassphraseB64: info.wrapped_passphrase_b64,
      });
      await runUnlock(email.trim(), recovered);
    } catch (err) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Passkey unlock failed');
      }
    } finally {
      setPasskeyBusy(false);
    }
  }

  const passkeyAvailable = isPasskeySupported();

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
              <span>encrypted at rest · self-hosted on home server</span>
            </div>
          </div>
        </div>

        <div className={styles.formPane}>
          <form className={styles.form} onSubmit={submit}>
            <div className={styles.brand}>S</div>

            <h1 className={styles.welcome}>Welcome back.</h1>
            <div className={styles.subhead}>
              {driveEmail
                ? `Signed in as ${driveEmail} via the drive — unlock your space.`
                : 'Sign in to your space.'}
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
                  disabled={busy || passkeyBusy}
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
                  autoComplete="current-password"
                  spellCheck={false}
                  disabled={busy || passkeyBusy}
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
              <div className={styles.hintRow}>
                <span className={styles.hintLeft}>
                  <span className={styles.dot} /> on this device only
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
              disabled={busy || passkeyBusy || !email.trim() || !passphrase}
            >
              {busy ? 'Opening…' : 'Open my space'}
              <Chevron size={14} />
            </button>

            {passkeyAvailable && (
              <button
                type="button"
                className={styles.passkeyAlt}
                onClick={tryPasskey}
                disabled={busy || passkeyBusy}
              >
                <span className={styles.passkeyIcon}>
                  <svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={1.6}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <rect x="3" y="11" width="18" height="11" rx="2" />
                    <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                  </svg>
                </span>
                <div className={styles.passkeyText}>
                  <div className={styles.passkeyTitle}>
                    {passkeyBusy ? 'Waiting for your authenticator…' : 'Or use a passkey'}
                  </div>
                  <div className={styles.passkeySub}>
                    {passkeyBusy ? 'Touch your security key or Touch ID' : 'Touch ID / hardware key'}
                  </div>
                </div>
                <Chevron size={13} />
              </button>
            )}

            {showRegisterLink && onRegister && (
              <button type="button" className={styles.registerAlt} onClick={onRegister}>
                Need a new account? Register
              </button>
            )}

            {DRIVE_URL && (
              <a className={styles.driveBack} href={DRIVE_URL}>
                ← Back to the cloud drive
              </a>
            )}

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
