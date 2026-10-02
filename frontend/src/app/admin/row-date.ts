import { formatDateTime, formatRelativeTime } from '@masmarino/gabarit';

/** A date as the admin lists show it: "il y a 3 j" up to a month, then "le 12/08/2026", with the exact time as the tooltip. */
export interface RowDate {
  iso: string;
  label: string;
  title: string;
}

export const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
export const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;

export function rowDate(iso: string, now: Date): RowDate {
  const relative = formatRelativeTime(iso, 'fr', now, RELATIVE_OPTIONS);
  return { iso, label: /^\d/.test(relative) ? `le ${relative}` : relative, title: formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS) };
}
