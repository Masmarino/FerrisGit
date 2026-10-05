import { DEFAULT_ACTIVATE_LABELS, DEFAULT_BACKUP_CODES_LABELS, DEFAULT_LOGIN_LABELS, DEFAULT_MFA_ENROLLMENT_LABELS, DEFAULT_MFA_SETTINGS_LABELS, DEFAULT_PASSKEY_SETTINGS_LABELS, DEFAULT_REGISTER_LABELS, DEFAULT_RESET_PASSWORD_LABELS, DEFAULT_TOTP_QR_LABELS } from '@masmarino/gabarit/auth';
import { FR_AUTH_LABELS } from './auth-labels.fr';

const DEFAULTS = {
  totpQr: DEFAULT_TOTP_QR_LABELS,
  backupCodes: DEFAULT_BACKUP_CODES_LABELS,
  mfaEnrollment: DEFAULT_MFA_ENROLLMENT_LABELS,
  login: DEFAULT_LOGIN_LABELS,
  register: DEFAULT_REGISTER_LABELS,
  activate: DEFAULT_ACTIVATE_LABELS,
  resetPassword: DEFAULT_RESET_PASSWORD_LABELS,
  mfaSettings: DEFAULT_MFA_SETTINGS_LABELS,
  passkeySettings: DEFAULT_PASSKEY_SETTINGS_LABELS,
} as const;

describe('FR_AUTH_LABELS', () => {
  it.each(Object.keys(DEFAULTS) as (keyof typeof DEFAULTS)[])('words every string of the kit\'s "%s" in French, and nothing the kit does not know', (part) => {
    const french = FR_AUTH_LABELS[part] as Record<string, unknown>;
    const english = DEFAULTS[part] as Record<string, unknown>;

    expect(Object.keys(french).sort()).toEqual(Object.keys(english).sort());
    for (const [key, value] of Object.entries(french)) {
      expect(typeof value, `${part}.${key}`).toBe(typeof english[key]);
      if (typeof value === 'string') {
        expect(value, `${part}.${key} is still the English default`).not.toBe(english[key]);
      }
    }
  });

  it('keeps the name and the title of the downloaded backup codes', () => {
    expect(FR_AUTH_LABELS.backupCodes.fileName).toBe('ferrisgit-codes-de-secours.txt');
    expect(FR_AUTH_LABELS.backupCodes.fileTitle).toBe('FerrisGit - codes de secours');
    expect(FR_AUTH_LABELS.backupCodes.fileNotice).toBe("Chaque code ne peut être utilisé qu'une seule fois. Conservez-les dans un endroit sûr.");
  });

  it('says the account cards signed the user out once a factor is gone', () => {
    expect(FR_AUTH_LABELS.mfaSettings.signedOut).toBe('Vous avez été déconnecté. Reconnectez-vous pour continuer.');
    expect(FR_AUTH_LABELS.passkeySettings.signedOut).toBe(FR_AUTH_LABELS.mfaSettings.signedOut);
  });

  describe('the worded values, as the pages said them', () => {
    it('the enrolment', () => {
      const labels = FR_AUTH_LABELS.mfaEnrollment;

      expect(labels.step(2, 3)).toBe('Étape 2 sur 3');
      expect(labels.codesLead('passkey')).toBe(
        "La double authentification est activée. Ces codes ne s'afficheront plus : ils vous permettent de vous connecter si vous perdez l'accès à votre clé d'accès, et chacun ne fonctionne qu'une seule fois.",
      );
      expect(labels.codesLead('totp')).toContain("l'accès à votre application, et");
      expect(labels.nameTooLong(40)).toBe('Le nom ne doit pas dépasser 40 caractères');
    });

    it('the registration and the activation', () => {
      expect(FR_AUTH_LABELS.register.passwordHint(8)).toBe('Au moins 8 caractères.');
      expect(FR_AUTH_LABELS.register.passwordTooShort(8)).toBe('Au moins 8 caractères');
      expect(FR_AUTH_LABELS.activate.passwordHint(8)).toBe('Au moins 8 caractères.');
      expect(FR_AUTH_LABELS.activate.weakPassword(8)).toBe('Le mot de passe doit comporter au moins 8 caractères');
      expect(FR_AUTH_LABELS.register.enrollmentExpired + FR_AUTH_LABELS.register.createdMessage).toBe(
        'La configuration a expiré. Connectez-vous avec votre mot de passe pour terminer la configuration de la double authentification, obligatoire avant de commencer.',
      );
    });

    it('the password reset, which says the old password no longer works (FerrisGit\'s server kills it at once)', () => {
      const labels = FR_AUTH_LABELS.resetPassword;

      expect(labels.intro).toBe(
        'Un administrateur a réinitialisé le mot de passe de votre compte : votre ancien mot de passe ne fonctionne plus. Choisissez-en un nouveau pour vous reconnecter.',
      );
      expect(labels.passwordHint(8)).toBe('Au moins 8 caractères.');
      expect(labels.weakPassword(8)).toBe('Le mot de passe doit comporter au moins 8 caractères');
      expect(labels.tooManyAttempts).toBe(FR_AUTH_LABELS.activate.tooManyAttempts);
    });

    it('the backup codes left', () => {
      const { codesLeft } = FR_AUTH_LABELS.mfaSettings;

      expect([codesLeft(0), codesLeft(1), codesLeft(7)]).toEqual(['Aucun code de secours restant', '1 code de secours restant', '7 codes de secours restants']);
    });

    it('the passkeys', () => {
      const labels = FR_AUTH_LABELS.passkeySettings;

      expect([labels.added(true), labels.added(false), labels.lastUsed(true), labels.lastUsed(false)]).toEqual(['Ajoutée le', 'Ajoutée', 'Dernière utilisation le', 'Dernière utilisation']);
      expect(labels.deleteKey('YubiKey')).toBe('Supprimer la clé YubiKey');
      expect(labels.passkeyAdded('YubiKey')).toBe("Clé d'accès « YubiKey » ajoutée.");
      expect(labels.passkeyGone('YubiKey')).toBe("La clé d'accès « YubiKey » n'existe plus.");
      expect(labels.deleteLead('YubiKey')).toBe('Confirmez votre mot de passe pour supprimer « YubiKey ».');
      expect(labels.deleteDialogMessage('YubiKey', false)).toBe('« YubiKey » ne permettra plus de vous connecter. Vous serez déconnecté de tous vos appareils et devrez vous reconnecter.');
      expect(labels.deleteDialogMessage('YubiKey', true)).toBe(
        "« YubiKey » ne permettra plus de vous connecter. Vous serez déconnecté de tous vos appareils et devrez vous reconnecter. C'est votre dernier facteur : vous devrez en configurer un nouveau à la prochaine connexion.",
      );
    });
  });
});
