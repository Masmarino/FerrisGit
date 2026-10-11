// English wording for the Gabarit auth kit: the kit's own English, except where FerrisGit says something more precise.
import type { AuthLabels } from '@masmarino/gabarit/auth';

export const EN_AUTH_LABELS = {
  backupCodes: {
    fileName: 'ferrisgit-backup-codes.txt',
    fileTitle: 'FerrisGit - backup codes',
  },
  login: {
    intro: 'Sign in to find your repositories, issues and merge requests.',
  },
  register: {
    intro: 'Join FerrisGit to host your repositories, issues and merge requests.',
    closedHeading: 'Sign-up is closed',
    closedMessage: 'This instance does not accept open sign-up. Ask an administrator for an invitation: you will receive a link by email to activate your account.',
  },
  activate: {
    invalidMessage:
      'This link is invalid or has expired. Use the latest email you received: a reminder with a new link is sent every day, until the account is deleted 7 days after it was created. After that, sign up again or ask an administrator for an invitation.',
  },
  // The kit's intro doesn't mention the old password, but our server voids it on an admin reset, so ours says so.
  resetPassword: {
    intro: 'An administrator has reset the password of your account: your old password no longer works. Choose a new one to sign in again.',
  },
} satisfies AuthLabels;
