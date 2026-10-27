/** Browser side of WebAuthn: converts between the server's JSON (base64url) and the browser API. */

const toBytes = (s: string): Uint8Array<ArrayBuffer> => {
  const b64 = s
    .replace(/-/g, '+')
    .replace(/_/g, '/')
    .padEnd(Math.ceil(s.length / 4) * 4, '=');
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
};
const fromBytes = (buf: ArrayBuffer): string => {
  let s = '';
  for (const b of new Uint8Array(buf)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

export const passkeysSupported = () =>
  typeof window !== 'undefined' &&
  !!window.PublicKeyCredential &&
  !!navigator.credentials;

type Options = Record<string, unknown> & {
  challenge: string;
  user?: { id: string; name: string; displayName: string };
  excludeCredentials?: { type: string; id: string }[];
  allowCredentials?: { type: string; id: string }[];
};

/** Asks the authenticator to create a passkey. Throws when the user cancels. */
export async function createPasskey(
  options: unknown,
): Promise<{ client_data_json: string; attestation_object: string }> {
  const o = options as Options;
  const cred = (await navigator.credentials.create({
    publicKey: {
      ...(o as object),
      challenge: toBytes(o.challenge),
      user: { ...o.user!, id: toBytes(o.user!.id) },
      excludeCredentials: (o.excludeCredentials ?? []).map((c) => ({
        type: 'public-key' as const,
        id: toBytes(c.id),
      })),
    } as unknown as PublicKeyCredentialCreationOptions,
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error('cancelled');
  const r = cred.response as AuthenticatorAttestationResponse;
  return {
    client_data_json: fromBytes(r.clientDataJSON),
    attestation_object: fromBytes(r.attestationObject),
  };
}

/** Asks the authenticator to sign the challenge with one of its passkeys. */
export async function getPasskey(options: unknown): Promise<{
  id: string;
  response: {
    client_data_json: string;
    authenticator_data: string;
    signature: string;
    user_handle?: string;
  };
}> {
  const o = options as Options;
  const cred = (await navigator.credentials.get({
    publicKey: {
      ...(o as object),
      challenge: toBytes(o.challenge),
      allowCredentials: (o.allowCredentials ?? []).map((c) => ({
        type: 'public-key' as const,
        id: toBytes(c.id),
      })),
    } as unknown as PublicKeyCredentialRequestOptions,
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error('cancelled');
  const r = cred.response as AuthenticatorAssertionResponse;
  return {
    id: fromBytes(cred.rawId),
    response: {
      client_data_json: fromBytes(r.clientDataJSON),
      authenticator_data: fromBytes(r.authenticatorData),
      signature: fromBytes(r.signature),
      user_handle: r.userHandle ? fromBytes(r.userHandle) : undefined,
    },
  };
}
