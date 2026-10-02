import { InjectionToken } from '@angular/core';

/** True on the public pages: a visitor can read but not act (no star button). */
export const READ_ONLY_REPOSITORY = new InjectionToken<boolean>('READ_ONLY_REPOSITORY', { providedIn: 'root', factory: () => false });
