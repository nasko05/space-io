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

const HKDF_INFO = 'hearth-passkey-wrap';
const IV_LENGTH = 12;

/**
 * Determine the effective RP ID for WebAuthn operations.
 *
 * The WebAuthn spec requires `rp.id` to be a "valid domain string". IP
 * addresses (including `127.0.0.1` and `::1`) are NOT valid domain strings
 * and will cause a SecurityError if explicitly passed. However, loopback
 * addresses are allowed when the browser derives the RP ID from the origin
 * itself (i.e., when `rp.id` is omitted).
 *
 * Returns `undefined` for loopback IPs (meaning: omit from options, let
 * browser default to the origin's effective domain). Returns the hostname
 * string for `localhost` and all other valid domain names.
 */
function getEffectiveRpId(): string | undefined {
  const host = window.location.hostname;
  // Loopback IPs: omit rp.id so the browser defaults to the effective domain
  // without triggering the "valid domain string" check.
  if (host === '127.0.0.1' || host === '::1') {
    return undefined;
  }
  // 'localhost' and regular domain names are valid RP IDs.
  return host;
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
        | 'ssr';
      message: string;
    };

function isInsideCrossOriginFrame(): boolean {
  try {
    return window.top !== null && window.top !== window;
  } catch {
    // Accessing window.top across origins throws — that's the definition.
    return true;
  }
}

/** Inspect the runtime for the conditions WebAuthn needs *before* we call
 *  into `navigator.credentials`. Surfacing this up-front lets the UI explain
 *  why the passkey path is unavailable instead of leaving the user to decode
 *  "The operation is insecure" out of a stack trace. */
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
  if (host === 'localhost' || host === '127.0.0.1' || host === '::1') {
    // Report the effective RP ID that will actually be used (or '<omitted>'
    // when we let the browser derive it from the origin).
    const effectiveRpId = getEffectiveRpId() ?? host;
    return { ok: true, origin, rpId: effectiveRpId, isLoopback: true };
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
  const s = webauthnStatus();
  if (!s.ok) throw new Error(s.message);
}

/** Wrap the underlying browser error so the modal sees something more
 *  useful than the bare DOMException's default message. NotAllowedError
 *  in particular is overloaded — it covers user cancel, timeout, the PRF
 *  extension being unsupported by the authenticator, and a few more.
 *  Also dumps the page context (origin + rp.id) so the user can paste it
 *  into a bug report without us having to ask "what URL are you on?". */
function wrapWebAuthnError(stage: 'create' | 'get', err: unknown): Error {
  // Pull the rp.id we tried + the origin so the message says exactly what
  // failed, not just "something failed".
  const origin = typeof window !== 'undefined' ? window.location.origin : '<no-window>';
  const rpId = typeof window !== 'undefined' ? (getEffectiveRpId() ?? '<browser-default>') : '<no-window>';

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
    // eslint-disable-next-line no-console
    console.error(
      `WebAuthn ${stage} failed:`,
      { name, origin, rpId, isSecureContext: window.isSecureContext, inFrame: isInsideCrossOriginFrame() },
      err,
    );
    return new Error(`${name}${hint}${detail}`);
  }
  // eslint-disable-next-line no-console
  console.error(`WebAuthn ${stage} failed:`, { origin, rpId }, err);
  return err instanceof Error ? err : new Error(String(err));
}

/** Create a new passkey and wrap the user's passphrase under its PRF secret. */
export async function registerPasskey(
  owner: string,
  passphrase: string,
): Promise<RegisterResult> {
  if (!isPasskeySupported()) {
    throw new Error('Passkeys are not available in this browser.');
  }
  ensureWebAuthnUsable();
  const prfSalt = crypto.getRandomValues(new Uint8Array(32));
  const challenge = crypto.getRandomValues(new Uint8Array(32));
  const userId = await sha256(new TextEncoder().encode(`hearth:${owner}`));

  const rpId = getEffectiveRpId();
  // Build the RP descriptor; omit `id` for loopback IPs so the browser
  // derives it from the origin (avoids SecurityError on IP addresses).
  const rp: { name: string; id?: string } = { name: 'SpaceIO' };
  if (rpId !== undefined) rp.id = rpId;

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
          { type: 'public-key', alg: -7 }, // ES256
          { type: 'public-key', alg: -257 }, // RS256
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
  if (!cred) throw new Error('passkey creation was cancelled');

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
  if (!isPasskeySupported()) {
    throw new Error('Passkeys are not available in this browser.');
  }
  ensureWebAuthnUsable();
  const credentialId = b64UrlToBytes(input.credentialIdB64);
  const prfSalt = b64UrlToBytes(input.prfSaltB64);
  const wrapped = b64UrlToBytes(input.wrappedPassphraseB64);
  const challenge = crypto.getRandomValues(new Uint8Array(32));

  const rpId = getEffectiveRpId();
  // Build assertion options; omit `rpId` for loopback IPs (same rationale
  // as registration — IP addresses are not valid RP IDs per the spec).
  const publicKeyBase = {
    challenge,
    allowCredentials: [{ id: credentialId, type: 'public-key' as const }],
    userVerification: 'preferred' as const,
    timeout: 60_000,
    extensions: { prf: { eval: { first: prfSalt } } },
  };
  const publicKeyOptions = rpId !== undefined
    ? { ...publicKeyBase, rpId }
    : publicKeyBase;

  let assertion: PublicKeyCredential | null;
  try {
    assertion = (await navigator.credentials.get({
      publicKey: publicKeyOptions,
    } as CredentialRequestOptions)) as PublicKeyCredential | null;
  } catch (err) {
    throw wrapWebAuthnError('get', err);
  }
  if (!assertion) throw new Error('passkey assertion was cancelled');

  const extensions = (assertion.getClientExtensionResults() as PrfExtensionResults).prf;
  const prfOutput = extensions?.results?.first;
  if (!prfOutput) {
    throw new Error('This authenticator did not return a PRF secret.');
  }
  return await unwrapPassphrase(prfOutput, wrapped);
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
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function b64UrlToBytes(s: string): Uint8Array {
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4));
  const normalised = s.replace(/-/g, '+').replace(/_/g, '/') + pad;
  const bin = atob(normalised);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

type PrfExtensionResults = AuthenticationExtensionsClientOutputs & {
  prf?: { results?: { first?: ArrayBuffer } };
};
