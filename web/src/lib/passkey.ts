/**
 * WebAuthn helpers using the PRF extension for end-to-end encryption.
 *
 * The server never sees the passphrase via this path: the browser uses the
 * PRF output as a stable secret tied to the authenticator + salt, derives a
 * 32-byte AES-GCM key via HKDF-SHA256, and encrypts the passphrase locally.
 * The server stores the resulting ciphertext + the PRF salt + the credential
 * ID, all of which are opaque to it.
 *
 * Requires browser support for the WebAuthn PRF extension (Chrome 116+,
 * Safari 17.4+, FIDO2 authenticators with hmac-secret).
 */

const HKDF_INFO = 'space-io-passkey-wrap';
const IV_LENGTH = 12;
const ES256_ALGORITHM = -7;
const RS256_ALGORITHM = -257;

/**
 * WebAuthn rejects IP-addressed origins (`rp.id` must be a registrable domain
 * suffix), so redirect a `127.0.0.1`/`::1` origin to `localhost` on the same
 * port. Call before any passkey operation.
 */
function ensureLocalhostOrigin(): void {
  const host = window.location.hostname;
  if (host === '127.0.0.1' || host === '::1') {
    const url = new URL(window.location.href);
    url.hostname = 'localhost';
    window.location.replace(url.toString());
  }
}

/**
 * RP ID for the ceremony. Defaults to the current hostname (standalone editor).
 * In a co-hosted "plug-in" deployment, set VITE_WEBAUTHN_RP_ID to the
 * registrable parent domain (e.g. `example.com`) so the single passkey
 * registered on the drive works from the editor's sibling subdomain too — a
 * passkey scoped to the parent domain is usable from any subdomain under it.
 */
function getEffectiveRpId(): string {
  const configured = (import.meta.env.VITE_WEBAUTHN_RP_ID as string | undefined)?.trim();
  return configured || window.location.hostname;
}

export interface RegisterResult {
  credentialIdB64: string;
  prfSaltB64: string;
  wrappedPassphraseB64: string;
}

export interface AuthenticateInput {
  credentialIdB64: string;
  prfSaltB64: string;
  wrappedPassphraseB64: string;
}

export function isPasskeySupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.PublicKeyCredential !== 'undefined' &&
    typeof navigator !== 'undefined' &&
    typeof navigator.credentials !== 'undefined'
  );
}

export type WebAuthnStatus =
  | { ok: true; origin: string; rpId: string; isLoopback: boolean }
  | {
      ok: false;
      reason:
        | 'unsupported'
        | 'insecure-context'
        | 'ip-host'
        | 'cross-origin-frame'
        | 'redirecting'
        | 'ssr';
      message: string;
    };

function isInsideCrossOriginFrame(): boolean {
  try {
    return window.top !== null && window.top !== window;
  } catch {
    return true;
  }
}

/** Check the conditions WebAuthn needs before calling `navigator.credentials`,
 *  so the UI can explain why the passkey path is unavailable rather than
 *  surfacing a raw "operation is insecure" error. */
export function webauthnStatus(): WebAuthnStatus {
  if (typeof window === 'undefined') {
    return { ok: false, reason: 'ssr', message: 'No window object (server render).' };
  }
  if (!isPasskeySupported()) {
    return {
      ok: false,
      reason: 'unsupported',
      message:
        'This browser does not expose the WebAuthn API. Try a recent Chrome, Edge, Safari, or Firefox.',
    };
  }
  if (!window.isSecureContext) {
    return {
      ok: false,
      reason: 'insecure-context',
      message:
        `Passkeys require a secure context. The current origin (${window.location.protocol}//${window.location.host}) is plain HTTP. Open the app over HTTPS, or reach it at 127.0.0.1 via the SSH tunnel.`,
    };
  }
  if (isInsideCrossOriginFrame()) {
    return {
      ok: false,
      reason: 'cross-origin-frame',
      message:
        'Passkeys cannot be used from a cross-origin iframe without an explicit `allow="publickey-credentials-create; publickey-credentials-get"` on the embedding frame. Open the app in its own tab.',
    };
  }
  const host = window.location.hostname;
  const origin = window.location.origin;
  if (host === '127.0.0.1' || host === '::1') {
    ensureLocalhostOrigin();
    return { ok: false, reason: 'redirecting', message: 'Redirecting to localhost for WebAuthn compatibility…' };
  }
  if (host === 'localhost') {
    return { ok: true, origin, rpId: host, isLoopback: true };
  }
  const isIpV4 = /^\d{1,3}(\.\d{1,3}){3}$/.test(host);
  const isIpV6 = host.includes(':') || host.startsWith('[');
  if (isIpV4 || isIpV6) {
    return {
      ok: false,
      reason: 'ip-host',
      message:
        `Passkeys need a domain name as the relying-party ID; '${host}' is an IP. Reach the app via a domain, or the SSH tunnel at 127.0.0.1.`,
    };
  }
  return { ok: true, origin, rpId: host, isLoopback: false };
}

/** Throw a single Error explaining why WebAuthn can't run, if it can't. */
function ensureWebAuthnUsable(): void {
  const status = webauthnStatus();
  if (!status.ok) { throw new Error(status.message); }
}

/** Turn the browser's overloaded WebAuthn errors (NotAllowedError covers
 *  cancel, timeout, unsupported PRF, …) into a specific message, with the
 *  origin + rp.id attached for bug reports. */
function wrapWebAuthnError(stage: 'create' | 'get', err: unknown): Error {
  const origin = typeof window !== 'undefined' ? window.location.origin : '<no-window>';
  const rpId = typeof window !== 'undefined' ? getEffectiveRpId() : '<no-window>';

  if (err instanceof DOMException) {
    const name = err.name;
    const detail = err.message ? ` (${err.message})` : '';
    let hint = '';
    if (name === 'NotAllowedError') {
      hint = stage === 'create'
        ? ' — cancelled, timed out, or the authenticator refused this site.'
        : ' — cancelled, timed out, or no matching credential on the authenticator.';
    } else if (name === 'SecurityError') {
      hint = ` — the browser rejected rp.id='${rpId}' for origin ${origin}. Common causes: the page is in a cross-origin iframe, rp.id is not a registrable suffix of the origin, or the origin is not actually secure despite isSecureContext.`;
    } else if (name === 'InvalidStateError') {
      hint = stage === 'create'
        ? ' — this authenticator already has a credential for this account.'
        : '';
    } else if (name === 'NotSupportedError') {
      hint = ' — the authenticator does not support the required algorithm or extension.';
    } else if (name === 'AbortError') {
      hint = ' — the request was aborted.';
    }
    console.error(
      `WebAuthn ${stage} failed:`,
      { name, origin, rpId, isSecureContext: window.isSecureContext, inFrame: isInsideCrossOriginFrame() },
      err,
    );
    return new Error(`${name}${hint}${detail}`);
  }
  console.error(`WebAuthn ${stage} failed:`, { origin, rpId }, err);
  return err instanceof Error ? err : new Error(String(err));
}

/** Create a new passkey and wrap the user's passphrase under its PRF secret. */
export async function registerPasskey(
  owner: string,
  passphrase: string,
): Promise<RegisterResult> {
  ensureLocalhostOrigin();
  if (!isPasskeySupported()) {
    throw new Error('Passkeys are not available in this browser.');
  }
  ensureWebAuthnUsable();
  const prfSalt = crypto.getRandomValues(new Uint8Array(32));
  const challenge = crypto.getRandomValues(new Uint8Array(32));
  const userId = await sha256(new TextEncoder().encode(`space-io:${owner}`));

  const rp = { name: 'SpaceIO', id: getEffectiveRpId() };

  let cred: PublicKeyCredential | null;
  try {
    cred = (await navigator.credentials.create({
      publicKey: {
        challenge,
        rp,
        user: {
          id: userId.slice(0, 16),
          name: owner,
          displayName: owner,
        },
        pubKeyCredParams: [
          { type: 'public-key', alg: ES256_ALGORITHM },
          { type: 'public-key', alg: RS256_ALGORITHM },
        ],
        authenticatorSelection: {
          userVerification: 'preferred',
          residentKey: 'preferred',
        },
        timeout: 60_000,
        attestation: 'none',
        extensions: { prf: { eval: { first: prfSalt } } },
      },
    } as CredentialCreationOptions)) as PublicKeyCredential | null;
  } catch (err) {
    throw wrapWebAuthnError('create', err);
  }
  if (!cred) { throw new Error('passkey creation was cancelled'); }

  const extensions = (cred.getClientExtensionResults() as PrfExtensionResults).prf;
  const prfOutput = extensions?.results?.first;
  if (!prfOutput) {
    throw new Error(
      'This authenticator did not return a PRF secret. Try a hardware key or update your browser.',
    );
  }
  const wrappedPassphrase = await wrapPassphrase(prfOutput, passphrase);
  return {
    credentialIdB64: bytesToB64Url(new Uint8Array(cred.rawId)),
    prfSaltB64: bytesToB64Url(prfSalt),
    wrappedPassphraseB64: bytesToB64Url(wrappedPassphrase),
  };
}

/** Use an existing passkey to recover the stored passphrase. */
export async function unlockWithPasskey(input: AuthenticateInput): Promise<string> {
  ensureLocalhostOrigin();
  if (!isPasskeySupported()) {
    throw new Error('Passkeys are not available in this browser.');
  }
  ensureWebAuthnUsable();
  const credentialId = b64UrlToBytes(input.credentialIdB64);
  const prfSalt = b64UrlToBytes(input.prfSaltB64);
  const wrapped = b64UrlToBytes(input.wrappedPassphraseB64);
  const challenge = crypto.getRandomValues(new Uint8Array(32));

  let assertion: PublicKeyCredential | null;
  try {
    assertion = (await navigator.credentials.get({
      publicKey: {
        challenge,
        rpId: getEffectiveRpId(),
        allowCredentials: [{ id: credentialId, type: 'public-key' as const }],
        userVerification: 'preferred' as const,
        timeout: 60_000,
        extensions: { prf: { eval: { first: prfSalt } } },
      },
    } as CredentialRequestOptions)) as PublicKeyCredential | null;
  } catch (err) {
    throw wrapWebAuthnError('get', err);
  }
  if (!assertion) { throw new Error('passkey assertion was cancelled'); }

  const extensions = (assertion.getClientExtensionResults() as PrfExtensionResults).prf;
  const prfOutput = extensions?.results?.first;
  if (!prfOutput) {
    throw new Error('This authenticator did not return a PRF secret.');
  }
  return await unwrapPassphrase(prfOutput, wrapped);
}

export interface SsoProvisionResult {
  credentialIdB64: string;
  prfSaltB64: string;
  wrappedPassphraseB64: string;
  /** The freshly generated space passphrase, to initialise the encrypted space.
   *  The user never sees or types it — the passkey unlocks it from here on. */
  passphrase: string;
}

/**
 * First-entry provisioning for a drive-authenticated user: use their existing
 * (drive-registered, discoverable) passkey to derive a PRF secret, generate a
 * fresh random space passphrase, and wrap it under that secret. `allowCredentials`
 * is empty so the browser lets the user pick their passkey; the chosen
 * credential id is captured so later unlocks target the same one.
 */
export async function ssoProvision(): Promise<SsoProvisionResult> {
  ensureLocalhostOrigin();
  if (!isPasskeySupported()) {
    throw new Error('Passkeys are not available in this browser.');
  }
  ensureWebAuthnUsable();
  const prfSalt = crypto.getRandomValues(new Uint8Array(32));
  const challenge = crypto.getRandomValues(new Uint8Array(32));
  const passphrase = randomPassphrase();

  let assertion: PublicKeyCredential | null;
  try {
    assertion = (await navigator.credentials.get({
      publicKey: {
        challenge,
        rpId: getEffectiveRpId(),
        allowCredentials: [],
        userVerification: 'preferred' as const,
        timeout: 60_000,
        extensions: { prf: { eval: { first: prfSalt } } },
      },
    } as CredentialRequestOptions)) as PublicKeyCredential | null;
  } catch (err) {
    throw wrapWebAuthnError('get', err);
  }
  if (!assertion) { throw new Error('passkey selection was cancelled'); }

  const extensions = (assertion.getClientExtensionResults() as PrfExtensionResults).prf;
  const prfOutput = extensions?.results?.first;
  if (!prfOutput) {
    throw new Error(
      'This passkey did not return a PRF secret. Register a passkey on the drive that supports PRF (hmac-secret), then try again.',
    );
  }
  const wrapped = await wrapPassphrase(prfOutput, passphrase);
  return {
    credentialIdB64: bytesToB64Url(new Uint8Array(assertion.rawId)),
    prfSaltB64: bytesToB64Url(prfSalt),
    wrappedPassphraseB64: bytesToB64Url(wrapped),
    passphrase,
  };
}

/** 32 bytes of entropy as base64url (~43 chars) — well past the server's
 *  12-char floor, and never shown to the user. */
function randomPassphrase(): string {
  return bytesToB64Url(crypto.getRandomValues(new Uint8Array(32)));
}

async function wrapPassphrase(prfOutput: ArrayBuffer, passphrase: string): Promise<Uint8Array> {
  const aesKey = await deriveAesKey(prfOutput);
  const iv = crypto.getRandomValues(new Uint8Array(IV_LENGTH));
  const ct = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv },
      aesKey,
      new TextEncoder().encode(passphrase),
    ),
  );
  const out = new Uint8Array(iv.byteLength + ct.byteLength);
  out.set(iv, 0);
  out.set(ct, iv.byteLength);
  return out;
}

async function unwrapPassphrase(prfOutput: ArrayBuffer, wrapped: Uint8Array): Promise<string> {
  const aesKey = await deriveAesKey(prfOutput);
  const iv = wrapped.slice(0, IV_LENGTH);
  const ct = wrapped.slice(IV_LENGTH);
  const pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, aesKey, ct);
  return new TextDecoder().decode(pt);
}

async function deriveAesKey(prfOutput: ArrayBuffer): Promise<CryptoKey> {
  const ikm = await crypto.subtle.importKey('raw', prfOutput, 'HKDF', false, ['deriveKey']);
  return await crypto.subtle.deriveKey(
    {
      name: 'HKDF',
      hash: 'SHA-256',
      salt: new Uint8Array(0),
      info: new TextEncoder().encode(HKDF_INFO),
    },
    ikm,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  );
}

async function sha256(data: Uint8Array): Promise<Uint8Array> {
  const buf = await crypto.subtle.digest('SHA-256', data as BufferSource);
  return new Uint8Array(buf);
}

function bytesToB64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) { binary += String.fromCharCode(byte); }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function b64UrlToBytes(encoded: string): Uint8Array {
  const pad = encoded.length % 4 === 0 ? '' : '='.repeat(4 - (encoded.length % 4));
  const normalised = encoded.replace(/-/g, '+').replace(/_/g, '/') + pad;
  const binary = atob(normalised);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) { out[i] = binary.charCodeAt(i); }
  return out;
}

type PrfExtensionResults = AuthenticationExtensionsClientOutputs & {
  prf?: { results?: { first?: ArrayBuffer } };
};
