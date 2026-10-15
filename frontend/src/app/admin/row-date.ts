import { formatDateTime } from '@masmarino/gabarit/format';
import { relativeWithOn } from '../shared/i18n/dated';
import { activeLocale } from '../shared/i18n/translator';

/** "3 days ago" up to a month, then "on 12/08/2026" (in the active language); the exact time goes in the tooltip. */
export interface RowDate {
  iso: string;
  label: string;
  title: string;
}

export const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
export const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;

export function rowDate(iso: string, now: Date): RowDate {
  return { iso, label: relativeWithOn(iso, now, RELATIVE_OPTIONS), title: formatDateTime(iso, activeLocale(), ABSOLUTE_OPTIONS) };
}
