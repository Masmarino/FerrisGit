/** Mirrors the server's account rules (`account_rules.rs`) to say what is wrong before the round trip. The server still has the final say. */

export const MIN_PASSWORD_LENGTH = 8;

export const USERNAME_HINT = '3 à 32 caractères, lettres, chiffres, - et _. Enregistré en minuscules.';
export const USERNAME_ERROR = 'Commencez par une lettre ; 3 à 32 caractères : lettres, chiffres, - et _';

const USERNAME = /^[A-Za-z][A-Za-z0-9_-]{2,31}$/;
const MAX_EMAIL_LENGTH = 254;

/** The server trims the name and stores it in lower case, so the check trims too and the pages show it in lower case (`accountName`). */
export function usernameError(raw: string): string | null {
  const username = raw.trim();
  if (username === '') {
    return "Saisissez un nom d'utilisateur";
  }
  return USERNAME.test(username) ? null : USERNAME_ERROR;
}

/** Same shape as `is_valid_mailbox`: one `@`, a local part, a dotted domain, no space, at most 254 characters. */
export function emailError(raw: string): string | null {
  const email = raw.trim();
  if (email === '') {
    return 'Saisissez votre adresse e-mail';
  }
  const parts = email.split('@');
  const [local, domain] = parts;
  const valid =
    email.length <= MAX_EMAIL_LENGTH &&
    !/\s/.test(email) &&
    parts.length === 2 &&
    local.length > 0 &&
    domain.includes('.') &&
    !domain.startsWith('.') &&
    !domain.endsWith('.');
  return valid ? null : 'Saisissez une adresse e-mail valide, par exemple nom@exemple.fr';
}

/** A password is never trimmed: spaces count. (The server counts bytes, so it is at least as lenient.) */
export function passwordError(password: string): string | null {
  if (password === '') {
    return 'Saisissez un mot de passe';
  }
  return password.length >= MIN_PASSWORD_LENGTH ? null : `Au moins ${MIN_PASSWORD_LENGTH} caractères`;
}

export function accountName(raw: string): string {
  return raw.trim().toLowerCase();
}
