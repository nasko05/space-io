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

/** Create a new passkey and wrap the user's passphrase under its PRF secret. */
export async function registerPasskey(
  owner: string,
  passphrase: string,
): Promise<RegisterResult> {
  if (!isPasskeySupported()) {
    throw new Error('Passkeys are not available in this browser.');
  }
  const prfSalt = crypto.getRandomValues(new Uint8Array(32));
  const challenge = crypto.getRandomValues(new Uint8Array(32));
  const userId = await sha256(new TextEncoder().encode(`hearth:${owner}`));

  const cred = (await navigator.credentials.create({
    publicKey: {
      challenge,
      rp: { name: 'SpaceIO', id: window.location.hostname },
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
  const credentialId = b64UrlToBytes(input.credentialIdB64);
  const prfSalt = b64UrlToBytes(input.prfSaltB64);
  const wrapped = b64UrlToBytes(input.wrappedPassphraseB64);
  const challenge = crypto.getRandomValues(new Uint8Array(32));

  const assertion = (await navigator.credentials.get({
    publicKey: {
      challenge,
      allowCredentials: [{ id: credentialId, type: 'public-key' }],
      userVerification: 'preferred',
      timeout: 60_000,
      extensions: { prf: { eval: { first: prfSalt } } },
    },
  } as CredentialRequestOptions)) as PublicKeyCredential | null;
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
