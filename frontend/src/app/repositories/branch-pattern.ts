import { t } from '../shared/i18n/translator';

/**
 * What the server would object to in a protected branch pattern (protected_branch.rs, `normalize_pattern`), in the
 * words of the form, or `null` for a valid or empty pattern. `*` is allowed: it stands for any run of characters.
 */
export function branchPatternProblem(pattern: string): string | null {
  if (pattern === '') {
    return null;
  }
  const invalid =
    pattern.length > 255 ||
    pattern.startsWith('refs/') ||
    /^[-/]|\/$/.test(pattern) ||
    pattern.includes('..') ||
    pattern.includes('//') ||
    pattern.endsWith('.lock') ||
    /[\s~^:?[\\\p{Cc}]/u.test(pattern);
  return invalid ? t('repositories.protectedBranches.patternProblem') : null;
}
