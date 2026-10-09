import { emailProblem, usernameProblem } from '@masmarino/gabarit/auth';
import { t } from '../shared/i18n/translator';

// Messages over Gabarit's account rules, which mirror the server's (`account_rules.rs`). The server decides in the end.

export const usernameHint = (): string => t('auth.username.hint');
export const usernameInvalidMessage = (): string => t('auth.username.invalid');

export function usernameError(raw: string): string | null {
  switch (usernameProblem(raw)) {
    case 'empty':
      return t('auth.username.empty');
    case 'invalid':
      return usernameInvalidMessage();
    default:
      return null;
  }
}

export function emailError(raw: string): string | null {
  switch (emailProblem(raw)) {
    case 'empty':
      return t('auth.email.empty');
    case 'invalid':
      return t('auth.email.invalid');
    default:
      return null;
  }
}
