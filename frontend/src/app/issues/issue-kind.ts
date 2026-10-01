import type { SelectOption } from '@masmarino/gabarit';
import type { Issue } from './issues.service';

export interface IssueKindPresentation {
  label: string;
  icon: string;
}

const KINDS: Record<Issue['kind'], IssueKindPresentation> = {
  bug: { label: 'Bug', icon: 'bug' },
  feature: { label: 'Fonctionnalité', icon: 'sparkles' },
  task: { label: 'Tâche', icon: 'square-check' },
  epic: { label: 'Epic', icon: 'layers' },
};

/** The one place that maps an issue kind to its label and icon. An unknown kind shows as its raw text with the generic icon. */
export function issueKindPresentation(kind: string): IssueKindPresentation {
  return Object.hasOwn(KINDS, kind) ? KINDS[kind as Issue['kind']] : { label: kind, icon: 'circle-dot' };
}

/** The kinds a new issue can be created with (epics are not created from the list). */
export type CreatableIssueKind = 'bug' | 'feature' | 'task';

export const ISSUE_KIND_OPTIONS: SelectOption<CreatableIssueKind>[] = (['bug', 'feature', 'task'] as const).map((value) => ({ value, label: KINDS[value].label }));
