// Specs and Storybook only: a browser whose WebAuthn is whatever the test wants.

export interface StubbedCredentials {
  create?: (options?: unknown) => Promise<unknown>;
  get?: (options?: unknown) => Promise<unknown>;
}

/** Replaces `navigator.credentials` and `PublicKeyCredential` (`null` for no WebAuthn) and returns the restore function. */
export function stubPasskeyBrowser(credentials: StubbedCredentials | null): () => void {
  const realCredentials = Object.getOwnPropertyDescriptor(navigator, 'credentials');
  const realConstructor = Object.getOwnPropertyDescriptor(globalThis, 'PublicKeyCredential');
  Object.defineProperty(navigator, 'credentials', { value: credentials ?? undefined, configurable: true, writable: true });
  Object.defineProperty(globalThis, 'PublicKeyCredential', { value: credentials ? class {} : undefined, configurable: true, writable: true });
  return () => {
    restore(navigator, 'credentials', realCredentials);
    restore(globalThis, 'PublicKeyCredential', realConstructor);
  };
}

function restore(target: object, key: string, descriptor: PropertyDescriptor | undefined): void {
  if (descriptor) {
    Object.defineProperty(target, key, descriptor);
  } else {
    delete (target as Record<string, unknown>)[key];
  }
}

const buffer = (...bytes: number[]) => new Uint8Array(bytes).buffer;

export function fakeAssertion(): unknown {
  return {
    id: 'credential-id',
    rawId: buffer(1, 2, 3),
    type: 'public-key',
    response: { authenticatorData: buffer(4, 5), clientDataJSON: buffer(6, 7), signature: buffer(8, 9), userHandle: null },
  };
}

export function fakeAttestation(): unknown {
  return {
    id: 'credential-id',
    rawId: buffer(1, 2, 3),
    type: 'public-key',
    response: { attestationObject: buffer(4, 5), clientDataJSON: buffer(6, 7) },
  };
}

export const REQUEST_OPTIONS = {
  challenge: 'AQIDBA',
  timeout: 300000,
  rpId: 'localhost',
  allowCredentials: [{ type: 'public-key', id: 'BQYH' }],
  userVerification: 'preferred',
};

export const CREATION_OPTIONS = {
  rp: { name: 'FerrisGit', id: 'localhost' },
  user: { id: 'CQoLDA', name: 'alice', displayName: 'alice' },
  challenge: 'AQIDBA',
  pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
  timeout: 300000,
  excludeCredentials: [],
  authenticatorSelection: { residentKey: 'discouraged', userVerification: 'preferred' },
  attestation: 'none',
};

export const dismissedPrompt = () => new DOMException('The operation either timed out or was not allowed.', 'NotAllowedError');
