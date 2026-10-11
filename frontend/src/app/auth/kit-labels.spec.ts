import { DEFAULT_ACTIVATE_LABELS, DEFAULT_BACKUP_CODES_LABELS, DEFAULT_LOGIN_LABELS, DEFAULT_MFA_ENROLLMENT_LABELS, DEFAULT_MFA_SETTINGS_LABELS, DEFAULT_PASSKEY_SETTINGS_LABELS, DEFAULT_REGISTER_LABELS, DEFAULT_RESET_PASSWORD_LABELS, DEFAULT_TOTP_QR_LABELS } from '@masmarino/gabarit/auth';
import { SUPPORTED_LANGUAGES } from '../shared/i18n/languages';
import { DE_AUTH_LABELS } from './auth-labels.de';
import { EN_AUTH_LABELS } from './auth-labels.en';
import { ES_AUTH_LABELS } from './auth-labels.es';
import { IT_AUTH_LABELS } from './auth-labels.it';
import { kitLabelsFor } from './kit-labels';

const DEFAULTS: Record<string, Record<string, unknown>> = {
  totpQr: DEFAULT_TOTP_QR_LABELS,
  backupCodes: DEFAULT_BACKUP_CODES_LABELS,
  mfaEnrollment: DEFAULT_MFA_ENROLLMENT_LABELS,
  login: DEFAULT_LOGIN_LABELS,
  register: DEFAULT_REGISTER_LABELS,
  activate: DEFAULT_ACTIVATE_LABELS,
  resetPassword: DEFAULT_RESET_PASSWORD_LABELS,
  mfaSettings: DEFAULT_MFA_SETTINGS_LABELS,
  passkeySettings: DEFAULT_PASSKEY_SETTINGS_LABELS,
};

describe('kit labels', () => {
  it('has a wording for every interface language', () => {
    for (const language of SUPPORTED_LANGUAGES) {
      expect(kitLabelsFor(language), language).toBeTruthy();
    }
  });

  describe.each([
    ['es', ES_AUTH_LABELS],
    ['it', IT_AUTH_LABELS],
    ['de', DE_AUTH_LABELS],
  ] as const)('"%s"', (_language, labels) => {
    it.each(Object.keys(DEFAULTS))('words every string of the kit\'s "%s", and nothing the kit does not know', (part) => {
      const translated = labels[part as keyof typeof labels] as Record<string, unknown>;
      const english = DEFAULTS[part];

      expect(Object.keys(translated).sort()).toEqual(Object.keys(english).sort());
      for (const [key, value] of Object.entries(translated)) {
        expect(typeof value, `${part}.${key}`).toBe(typeof english[key]);
      }
    });

    it('names the downloaded backup codes after FerrisGit', () => {
      expect(labels.backupCodes.fileName).toMatch(/^ferrisgit-.+\.txt$/);
      expect(labels.backupCodes.fileTitle).toMatch(/^FerrisGit - /);
    });
  });

  it('overrides only strings the kit knows in English, keeping the rest of its English', () => {
    for (const [part, overrides] of Object.entries(EN_AUTH_LABELS)) {
      for (const [key, value] of Object.entries(overrides)) {
        expect(typeof DEFAULTS[part][key], `${part}.${key}`).toBe(typeof value);
      }
    }
    expect(EN_AUTH_LABELS.backupCodes.fileName).toBe('ferrisgit-backup-codes.txt');
  });
});
