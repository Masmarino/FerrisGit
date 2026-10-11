import { formatRelativeTime } from '@masmarino/gabarit/format';
import { activeLocale, t } from './translator';

type RelativeOptions = NonNullable<Parameters<typeof formatRelativeTime>[3]> & { absoluteAfterDays: number };

const DAY = 86_400_000;

/**
 * Whether `formatRelativeTime` writes `iso` as a date rather than as a time ago, by Gabarit's own rule: the date takes
 * over once the gap reaches `absoluteAfterDays`. Only then does the date need the language's "on".
 */
export function isAbsoluteDate(iso: string, now: Date | undefined, absoluteAfterDays: number): boolean {
  return Math.abs((now ?? new Date()).getTime() - Date.parse(iso)) >= absoluteAfterDays * DAY;
}

/** "3 days ago", or "on 12/08/2026" once it is a date. */
export function relativeWithOn(iso: string, now: Date | undefined, options: RelativeOptions): string {
  const when = formatRelativeTime(iso, activeLocale(), now, options);
  return isAbsoluteDate(iso, now, options.absoluteAfterDays) ? `${t('common.dateOn')}${when}` : when;
}
