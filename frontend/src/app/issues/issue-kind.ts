import type { SelectOption } from '@masmarino/gabarit/select';
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

/** The one place mapping an issue kind to its label and icon. An unknown kind shows its raw text with the generic icon. */
export function issueKindPresentation(kind: string): IssueKindPresentation {
  return Object.hasOwn(KINDS, kind) ? KINDS[kind as Issue['kind']] : { label: kind, icon: 'circle-dot' };
}

/** Kinds a new issue can have (epics aren't created from the list). */
export type CreatableIssueKind = 'bug' | 'feature' | 'task';

export const ISSUE_KIND_OPTIONS: SelectOption<CreatableIssueKind>[] = (['bug', 'feature', 'task'] as const).map((value) => ({ value, label: KINDS[value].label }));
