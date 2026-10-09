import type { SelectOption } from '@masmarino/gabarit/select';
import type { Issue } from './issues.service';
import { t } from '../shared/i18n/translator';

export interface IssueKindPresentation {
  label: string;
  icon: string;
}

const KINDS: Record<Issue['kind'], IssueKindPresentation> = {
  bug: {
    get label() {
      return t('issues.kinds.bug');
    },
    icon: 'bug',
  },
  feature: {
    get label() {
      return t('issues.kinds.feature');
    },
    icon: 'sparkles',
  },
  task: {
    get label() {
      return t('issues.kinds.task');
    },
    icon: 'square-check',
  },
  epic: {
    get label() {
      return t('issues.kinds.epic');
    },
    icon: 'layers',
  },
};

/** The one place mapping an issue kind to its label and icon. An unknown kind shows its raw text with the generic icon. */
export function issueKindPresentation(kind: string): IssueKindPresentation {
  return Object.hasOwn(KINDS, kind) ? KINDS[kind as Issue['kind']] : { label: kind, icon: 'circle-dot' };
}

/** Kinds a new issue can have (epics aren't created from the list). */
export type CreatableIssueKind = 'bug' | 'feature' | 'task';

export const issueKindOptions = (): SelectOption<CreatableIssueKind>[] => (['bug', 'feature', 'task'] as const).map((value) => ({ value, label: KINDS[value].label }));
