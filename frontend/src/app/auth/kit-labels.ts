import type { AuthLabels } from '@masmarino/gabarit/auth';
import { Language } from '../shared/i18n/languages';
import { DE_AUTH_LABELS } from './auth-labels.de';
import { EN_AUTH_LABELS } from './auth-labels.en';
import { ES_AUTH_LABELS } from './auth-labels.es';
import { FR_AUTH_LABELS } from './auth-labels.fr';
import { IT_AUTH_LABELS } from './auth-labels.it';

/**
 * The wording of Gabarit's auth kit in each language. It stays in code rather than in the language files, like
 * ArtiFerris: some labels are functions (`step(current, total)`), and a language that is missing one gets the kit's
 * English default instead of a key.
 */
const LABELS: Record<Language, AuthLabels> = {
  fr: FR_AUTH_LABELS,
  en: EN_AUTH_LABELS,
  es: ES_AUTH_LABELS,
  it: IT_AUTH_LABELS,
  de: DE_AUTH_LABELS,
};

export function kitLabelsFor(language: Language): AuthLabels {
  return LABELS[language];
}
