import { HttpErrorResponse } from '@angular/common/http';

export type RegisterFailure =
  | 'disabled'
  | 'invalid'
  | 'username-invalid'
  | 'email-invalid'
  | 'password-weak'
  | 'username-reserved'
  | 'username-taken'
  | 'email-taken'
  | 'rate-limited'
  | 'other';

export type ActivateFailure = 'invalid-link' | 'weak-password' | 'rate-limited' | 'other';

const REGISTRATION_DISABLED = 'registration is disabled';
const USERNAME_RESERVED = 'username is reserved';
const EMAIL_TAKEN = 'email already in use';
const WEAK_PASSWORD_PREFIX = 'password must be at least';
const USERNAME_PREFIX = 'username ';
const INVALID_EMAIL = 'email is not a valid address';

export const REGISTER_INVALID_MESSAGE = 'Vérifiez les champs';
export const REGISTER_RESERVED_MESSAGE = "Ce nom d'utilisateur n'est pas disponible";
export const REGISTER_TAKEN_MESSAGE = "Ce nom d'utilisateur ou cette adresse e-mail est déjà utilisé";

/** Only the exact "disabled" 400 body means the instance is closed. An unreadable body should not tell a user that sign-ups are closed. */
export function classifyRegisterFailure(err: unknown): RegisterFailure {
  if (!(err instanceof HttpErrorResponse)) {
    return 'other';
  }
  const message = bodyMessage(err.error);
  switch (err.status) {
    case 400:
      if (message === REGISTRATION_DISABLED) {
        return 'disabled';
      }
      if (message === USERNAME_RESERVED) {
        return 'username-reserved';
      }
      if (message === INVALID_EMAIL) {
        return 'email-invalid';
      }
      if (message?.startsWith(WEAK_PASSWORD_PREFIX)) {
        return 'password-weak';
      }
      return message?.startsWith(USERNAME_PREFIX) ? 'username-invalid' : 'invalid';
    case 409:
      return message === EMAIL_TAKEN ? 'email-taken' : 'username-taken';
    case 429:
      return 'rate-limited';
    default:
      return 'other';
  }
}

/** One generic 400 for an unknown, expired or used token, a distinct one for a weak password (the token is then still good). */
export function classifyActivateFailure(err: unknown): ActivateFailure {
  if (!(err instanceof HttpErrorResponse)) {
    return 'other';
  }
  switch (err.status) {
    case 400:
      return bodyMessage(err.error)?.startsWith(WEAK_PASSWORD_PREFIX) ? 'weak-password' : 'invalid-link';
    case 429:
      return 'rate-limited';
    default:
      return 'other';
  }
}

function bodyMessage(body: unknown): string | null {
  if (typeof body === 'object' && body !== null && 'error' in body && typeof (body as { error: unknown }).error === 'string') {
    return (body as { error: string }).error;
  }
  return null;
}
