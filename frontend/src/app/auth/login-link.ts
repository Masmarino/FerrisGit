import { Params } from '@angular/router';

export interface LoginLink {
  commands: string[];
  queryParams: Params | null;
}

/** The sign-in page, coming back to `url` afterwards. The root needs no return, signing in lands on the dashboard anyway. */
export function loginLink(url: string): LoginLink {
  const back = url === '/' || url.startsWith('/login') ? null : url;
  return { commands: ['/login'], queryParams: back ? { returnUrl: back } : null };
}

/** Only follow a `returnUrl` that stays on this site: one leading slash, no scheme, no `//host`, no `/\host`. */
export function safeReturnUrl(value: string | null): string | null {
  if (!value || !value.startsWith('/') || value.startsWith('//') || value.startsWith('/\\') || value.startsWith('/login')) {
    return null;
  }
  return value;
}
