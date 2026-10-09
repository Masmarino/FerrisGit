import { t } from '../shared/i18n/translator';

// Gabarit classifies the failure; only the messages for the sign-up and invitation forms live here.
export { classifyRegisterFailure, type RegisterFailure } from '@masmarino/gabarit/auth';

export const registerInvalidMessage = (): string => t('auth.register.invalid');
export const registerReservedMessage = (): string => t('auth.username.reserved');
export const registerTakenMessage = (): string => t('auth.register.taken');
