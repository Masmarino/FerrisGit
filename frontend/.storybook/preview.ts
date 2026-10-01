import { LOCALE_ID } from '@angular/core';
import { applicationConfig, type Preview } from '@storybook/angular-vite';

const preview: Preview = {
  parameters: {
    controls: { expanded: true },
  },
  // Gabarit's format pipes (gbtRelativeTime, gbtDateTime, gbtBytes) fall back to Angular's
  // LOCALE_ID, 'en-US' by default; the app itself sets it in app.config.ts. Every story needs it too.
  decorators: [applicationConfig({ providers: [{ provide: LOCALE_ID, useValue: 'fr' }] })],
};

export default preview;
