import { inject, provideAppInitializer } from '@angular/core';
import { IconRegistry } from '@masmarino/gabarit';

/** The icons Gabarit does not ship. `search` is also in Gabarit's set; this one overrides it for a different stroke. */
const FERRISGIT_ICONS: Record<string, string> = {
  'folder-git-2': `<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/><circle cx="13" cy="13" r="2"/><path d="M13 15v3"/>`,
  search: `<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>`,
  'git-pull-request': `<circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 0 1 2 2v7"/><line x1="6" y1="9" x2="6" y2="21"/>`,
  'git-commit': `<circle cx="12" cy="12" r="3"/><path d="M3 12h6"/><path d="M15 12h6"/>`,
  'git-merge': `<circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M6 21V9a9 9 0 0 0 9 9"/>`,
  'git-branch': `<line x1="6" x2="6" y1="3" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>`,
  // Issue statuses: a ring that fills up (to do, in progress, in review, done).
  'circle-half': `<circle cx="12" cy="12" r="10"/><path d="M12 12V6a6 6 0 0 1 0 12z" fill="currentColor" stroke="none"/>`,
  'circle-three-quarters': `<circle cx="12" cy="12" r="10"/><path d="M12 12V6a6 6 0 1 1-6 6z" fill="currentColor" stroke="none"/>`,
  'git-pull-request-closed': `<circle cx="6" cy="6" r="3"/><path d="M6 9v12"/><path d="m21 3-6 6"/><path d="m21 9-6-6"/><path d="M18 11.5V15"/><circle cx="18" cy="18" r="3"/>`,
  'shield-plus': `<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="M9 12h6"/><path d="M12 9v6"/>`,
  'shield-off': `<path d="m2 2 20 20"/><path d="M5 5a1 1 0 0 0-1 1v7c0 5 3.5 7.5 7.67 8.94a1 1 0 0 0 .67.01c2.35-.82 4.48-1.97 5.9-3.71"/><path d="M9.309 3.652A12.252 12.252 0 0 0 11.24 2.28a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1v7a9.8 9.8 0 0 1-.08 1.264"/>`,
};

export const provideFerrisgitIcons = () =>
  provideAppInitializer(() => {
    inject(IconRegistry).registerAll(FERRISGIT_ICONS);
  });
