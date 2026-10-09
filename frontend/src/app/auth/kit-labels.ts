import type { AuthLabels } from '@masmarino/gabarit/auth';
import { Language } from '../shared/i18n/languages';
import { FR_AUTH_LABELS } from './auth-labels.fr';

/**
 * The wording of Gabarit's auth kit in each language. It stays in code rather than in the language files, like
 * ArtiFerris: some labels are functions (`step(current, total)`), and a language that is missing one gets the kit's
 * English default instead of a key.
 */
const LABELS: Record<Language, AuthLabels> = {
  fr: FR_AUTH_LABELS,
};

export function kitLabelsFor(language: Language): AuthLabels {
  return LABELS[language];
}
