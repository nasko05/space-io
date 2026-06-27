# space-io editor ⇄ cloud drive integration

This editor can run **standalone** (its own email + passphrase/passkey login) or
as an **opt-in plug-in** for the [cloud drive](https://github.com/nasko05/cloud-storage-system),
where one email-less passkey signs you into both and unlocks your encrypted
space with a single tap — no passphrase ever typed.

**With no SSO env set, none of this activates: the editor behaves exactly as the
standalone app.**

## How the plug-in mode works

1. You sign into the **drive** with a passkey. The drive issues a JWT and mirrors
   it into a cookie on the parent domain.
2. You open the editor (`personal-area.<domain>`). It verifies that cookie
   (`GET /api/auth/sso`) to learn who you are — no email typed.
3. One passkey tap on **"Open My Space"**:
   - **First time** → the browser generates a random space passphrase, derives a
     key from the passkey's **PRF** secret, wraps the passphrase, and provisions
     the encrypted space (`POST /api/auth/sso/provision`).
   - **Return visits** → the wrapped passphrase is fetched
     (`GET /api/auth/sso/space`), unwrapped via PRF, and the space unlocks
     (`POST /api/auth/sso/unlock`).
4. The space is keyed by the drive's stable user id (`sub`), so it survives email
   changes and needs no email→id mapping.

The server only ever holds the passphrase in its in-memory session (exactly as
the standalone unlock does); the key is derived client-side from the passkey.

**Login is passkey-only.** In plug-in mode the editor never shows an
email/password form. Open it without a drive session and it routes you to the
drive's email-less passkey login; the SSO cookie then brings you back to the
one-tap unlock. The editor's own email + passphrase login only appears in the
**standalone** editor (no `VITE_DRIVE_URL` configured).

## Environment knobs (editor side)

All optional; unset = standalone editor.

| Variable | Purpose |
|---|---|
| `SPACEIO_SSO_JWT_SECRET` | The drive's `DRIVE_SECRET_KEY`. Enables SSO; verifies the drive's HS256 token. Unset = SSO off. |
| `SPACEIO_SSO_COOKIE_NAME` | Cookie name (default `drive_sso`); must match the drive's `DRIVE_SSO_COOKIE_NAME`. |
| `VITE_WEBAUTHN_RP_ID` *(web build)* | The **parent** domain (e.g. `example.com`), matching the drive's `DRIVE_WEBAUTHN_RP_ID`, so the one passkey works here. Empty = the editor's own hostname (standalone). |
| `VITE_DRIVE_URL` *(web build)* | Full URL of the drive; when set, a **"Cloud drive"** link appears in the editor. |

## Re-enrollment note

PRF must be present when a passkey is *created*. A passkey registered before the
integration can't derive a key, so the editor's "Open My Space" tap will report a
missing PRF secret. Register a fresh, PRF-capable passkey on the drive once.

## End-to-end manual test checklist

CI covers the units and the SSO endpoints, but the real cross-subdomain passkey
ceremony needs a browser, HTTPS, and a real authenticator. To verify a paired
deployment:

1. **Config both apps:** same `DRIVE_SECRET_KEY` / `SPACEIO_SSO_JWT_SECRET`;
   `DRIVE_WEBAUTHN_RP_ID` = `VITE_WEBAUTHN_RP_ID` = the parent domain;
   `DRIVE_SSO_COOKIE_DOMAIN` = `.<parent>`; cross-link URLs set.
2. **Register** a new (PRF-capable) passkey while signed into the drive.
3. **Log out, then sign in** to the drive with that passkey (email-less). ✅ you're in.
4. Click **"My Space"** → editor → **"Open My Space"** → one passkey tap → your
   encrypted space provisions and the reader opens. ✅
5. **Lock** (the editor's lock control) and reload → one tap unlocks the same space. ✅
6. From the editor, click **"Cloud drive"** → back on the drive, still signed in. ✅
7. **Log out on the drive** → reopen the editor → it no longer auto-recognizes you. ✅
8. **Standalone check:** unset `SPACEIO_SSO_JWT_SECRET` (editor) and
   `DRIVE_SSO_COOKIE_DOMAIN` (drive) → each app works entirely on its own. ✅

See the drive's `INTEGRATION.md` for the host side of the contract.
