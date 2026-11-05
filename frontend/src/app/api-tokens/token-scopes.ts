import { t } from '../shared/i18n/translator';

export type ScopeArea = 'repositories' | 'issues' | 'merge-requests' | 'pipelines' | 'releases' | 'admin';
export type ScopeChoice = 'none' | 'read' | 'write';

/** In the server's order. Only an administrator's tokens can hold `admin`, so the form leaves it out for anyone else. */
export const SCOPE_AREAS: readonly ScopeArea[] = ['repositories', 'issues', 'merge-requests', 'pipelines', 'releases', 'admin'];

export function areaName(area: ScopeArea): string {
  switch (area) {
    case 'repositories':
      return t('accessTokens.scopes.repositories');
    case 'issues':
      return t('accessTokens.scopes.issues');
    case 'merge-requests':
      return t('accessTokens.scopes.mergeRequests');
    case 'pipelines':
      return t('accessTokens.scopes.pipelines');
    case 'releases':
      return t('accessTokens.scopes.releases');
    case 'admin':
      return t('accessTokens.scopes.admin');
  }
}

export function areaHint(area: ScopeArea): string {
  switch (area) {
    case 'repositories':
      return t('accessTokens.scopes.repositoriesHint');
    case 'issues':
      return t('accessTokens.scopes.issuesHint');
    case 'merge-requests':
      return t('accessTokens.scopes.mergeRequestsHint');
    case 'pipelines':
      return t('accessTokens.scopes.pipelinesHint');
    case 'releases':
      return t('accessTokens.scopes.releasesHint');
    case 'admin':
      return t('accessTokens.scopes.adminHint');
  }
}

/** One scope per area on the wire, since `write` includes `read`. */
export function scopesFromChoices(areas: readonly ScopeArea[], choices: Record<ScopeArea, ScopeChoice>): string[] {
  return areas.filter((area) => choices[area] !== 'none').map((area) => `${area}:${choices[area]}`);
}

export interface ScopeDescription {
  label: string;
  hint: string | null;
}

/**
 * A scope in plain words. One this version doesn't know stays as it was written: whoever confirms a terminal has to see
 * everything it asks for.
 */
export function describeScope(scope: string): ScopeDescription {
  const [area, level] = scope.split(':') as [ScopeArea, string];
  if (!SCOPE_AREAS.includes(area) || (level !== 'read' && level !== 'write')) {
    return { label: scope, hint: null };
  }
  const name = areaName(area);
  return { label: level === 'write' ? t('accessTokens.scopes.readWriteOn', { area: name }) : t('accessTokens.scopes.readOn', { area: name }), hint: areaHint(area) };
}
