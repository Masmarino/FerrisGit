import { InjectionToken } from '@angular/core';

/** True on the public pages, where a visitor can read a repository but not act on it (no star button). */
export const READ_ONLY_REPOSITORY = new InjectionToken<boolean>('READ_ONLY_REPOSITORY', { providedIn: 'root', factory: () => false });
