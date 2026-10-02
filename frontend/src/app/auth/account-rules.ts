import { emailProblem, usernameProblem } from '@masmarino/gabarit';

// French messages over Gabarit's account rules, which mirror the server's (`account_rules.rs`). The server decides in the end.

export const USERNAME_HINT = '3 à 32 caractères, lettres, chiffres, - et _. Enregistré en minuscules.';
export const USERNAME_ERROR = 'Commencez par une lettre ; 3 à 32 caractères : lettres, chiffres, - et _';

export function usernameError(raw: string): string | null {
  switch (usernameProblem(raw)) {
    case 'empty':
      return "Saisissez un nom d'utilisateur";
    case 'invalid':
      return USERNAME_ERROR;
    default:
      return null;
  }
}

export function emailError(raw: string): string | null {
  switch (emailProblem(raw)) {
    case 'empty':
      return 'Saisissez votre adresse e-mail';
    case 'invalid':
      return 'Saisissez une adresse e-mail valide, par exemple nom@exemple.fr';
    default:
      return null;
  }
}
