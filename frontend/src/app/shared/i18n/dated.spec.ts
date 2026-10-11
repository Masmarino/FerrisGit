import { isAbsoluteDate, relativeWithOn } from './dated';
import { setActiveLanguage } from './translator';

const NOW = new Date('2026-10-11T12:00:00Z');
const OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;

describe('dated', () => {
  afterEach(() => setActiveLanguage('fr'));

  it('switches to a date at the same gap as Gabarit', () => {
    expect(isAbsoluteDate('2026-09-12T12:00:01Z', NOW, 30)).toBe(false);
    expect(isAbsoluteDate('2026-09-11T12:00:00Z', NOW, 30)).toBe(true);
  });

  it('puts the "on" before a date only, whatever the language writes first', () => {
    expect(relativeWithOn('2026-10-08T12:00:00Z', NOW, OPTIONS)).not.toMatch(/^le /);
    expect(relativeWithOn('2026-08-01T12:00:00Z', NOW, OPTIONS)).toMatch(/^le 01\/08\/2026$/);
  });
});
