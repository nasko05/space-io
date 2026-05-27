import { FormEvent, useState } from 'react';
import { Close, Eye } from '../icons/Icon';
import { api, ApiError } from '../../api/client';
import { isPasskeySupported, registerPasskey, unlockWithPasskey } from '../../lib/passkey';
import styles from './PasskeyModal.module.css';

interface Props {
  open: boolean;
  /** Email of the currently-unlocked user; needed to re-verify the passphrase. */
  email: string;
  /** Display name (for the WebAuthn `user.name` field). */
  owner: string;
  hasPasskey: boolean;
  onClose: () => void;
  onChanged: () => void;
}

type Phase =
  | { kind: 'idle' }
  | { kind: 'verifying' }
  | { kind: 'registering' }
  | { kind: 'removing' }
  | { kind: 'done'; message: string }
  | { kind: 'error'; message: string };

export function PasskeyModal({ open, email, owner, hasPasskey, onClose, onChanged }: Props) {
  const [passphrase, setPassphrase] = useState('');
  const [visible, setVisible] = useState(false);
  const [phase, setPhase] = useState<Phase>({ kind: 'idle' });

  if (!open) return null;

  async function register(e: FormEvent) {
    e.preventDefault();
    if (!passphrase) return;
    setPhase({ kind: 'verifying' });

    // First confirm the passphrase is correct by trying unlock (the server
    // will mint a new session cookie either way; we accept that because the
    // user is already in an unlocked session anyway).
    try {
      await api.unlock(email, passphrase);
    } catch (err) {
      const msg =
        err instanceof ApiError && err.code === 'wrong_passphrase'
          ? 'Wrong passphrase'
          : err instanceof Error
          ? err.message
          : 'Could not verify';
      setPhase({ kind: 'error', message: msg });
      return;
    }

    setPhase({ kind: 'registering' });
    try {
      const reg = await registerPasskey(owner, passphrase);
      // Sanity-check that we can actually decrypt what we just wrote.
      const recovered = await unlockWithPasskey({
        credentialIdB64: reg.credentialIdB64,
        prfSaltB64: reg.prfSaltB64,
        wrappedPassphraseB64: reg.wrappedPassphraseB64,
      });
      if (recovered !== passphrase) {
        throw new Error('Self-check failed: recovered passphrase did not match.');
      }
      await api.registerPasskey({
        credential_id_b64: reg.credentialIdB64,
        prf_salt_b64: reg.prfSaltB64,
        wrapped_passphrase_b64: reg.wrappedPassphraseB64,
      });
      setPhase({ kind: 'done', message: 'Passkey registered. You can unlock with it next time.' });
      setPassphrase('');
      onChanged();
    } catch (err) {
      setPhase({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Registration failed',
      });
    }
  }

  async function remove() {
    if (!confirm('Remove the passkey from this space?')) return;
    setPhase({ kind: 'removing' });
    try {
      await api.deletePasskey();
      setPhase({ kind: 'done', message: 'Passkey removed.' });
      onChanged();
    } catch (err) {
      setPhase({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Could not remove passkey',
      });
    }
  }

  const supported = isPasskeySupported();

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>Passkey</h2>
            <div className={styles.sub}>
              An alternate way to unlock — Touch&nbsp;ID, Windows&nbsp;Hello, or a hardware key.
              <br />
              The server still sees only ciphertext; the passkey decrypts your passphrase locally.
            </div>
          </div>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={16} />
          </button>
        </div>

        {!supported && (
          <div className={styles.warn}>
            Your browser doesn't expose the WebAuthn PRF extension that Hearth needs to wrap your
            passphrase. Try a recent Chrome, Edge, or Safari on a device with Touch ID or a
            FIDO2 key.
          </div>
        )}

        {hasPasskey ? (
          <div className={styles.state}>
            <div className={styles.dot} />
            <div className={styles.stateText}>
              <div className={styles.stateTitle}>Passkey active</div>
              <div className={styles.stateSub}>You'll see "Or use a passkey" on the lock screen.</div>
            </div>
            <button
              type="button"
              className={styles.removeBtn}
              onClick={remove}
              disabled={phase.kind === 'removing'}
            >
              {phase.kind === 'removing' ? 'Removing…' : 'Remove'}
            </button>
          </div>
        ) : (
          <form className={styles.form} onSubmit={register}>
            <div className={styles.label}>Confirm your passphrase to enrol a passkey</div>
            <div className={styles.passWrap}>
              <input
                className={styles.passInput}
                type={visible ? 'text' : 'password'}
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                autoComplete="current-password"
                spellCheck={false}
                disabled={phase.kind === 'verifying' || phase.kind === 'registering'}
                placeholder="••••••••••"
                autoFocus
              />
              <button
                type="button"
                className={styles.eyeBtn}
                onClick={() => setVisible((v) => !v)}
                tabIndex={-1}
                aria-label={visible ? 'Hide' : 'Show'}
              >
                <Eye size={15} />
              </button>
            </div>

            <div className={styles.actions}>
              <button type="button" className={styles.cancel} onClick={onClose}>
                Cancel
              </button>
              <button
                type="submit"
                className={styles.submit}
                disabled={!supported || !passphrase || phase.kind === 'verifying' || phase.kind === 'registering'}
              >
                {phase.kind === 'verifying'
                  ? 'Checking…'
                  : phase.kind === 'registering'
                  ? 'Talking to your authenticator…'
                  : 'Enrol passkey'}
              </button>
            </div>
          </form>
        )}

        {phase.kind === 'error' && <div className={styles.error}>{phase.message}</div>}
        {phase.kind === 'done' && <div className={styles.success}>{phase.message}</div>}
      </div>
    </div>
  );
}
