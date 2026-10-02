// Storybook-only fixtures shared by the issue stories (not imported by the app).
import { Label } from '../labels/labels.service';
import { Milestone } from '../milestones/milestones.service';
import { UserRef } from '../shared/user-ref';

export const label = (id: string, name: string, color: string): Label => ({ id, name, color, repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' });

export const LABELS: Label[] = [
  label('l-bug', 'bug', '#dc2626'),
  label('l-urgent', 'urgent', '#f97316'),
  label('l-ui', 'interface', '#6366f1'),
  label('l-docs', 'documentation', '#0ea5e9'),
  label('l-perf', 'performance', '#16a34a'),
  label('l-good-first', 'bon premier ticket', '#a855f7'),
];
export const [BUG, URGENT, UI, DOCS, PERF, GOOD_FIRST] = LABELS;

export const MILESTONES: Milestone[] = [
  { id: 'm1', title: 'v1.0', description: 'Première version stable', dueDate: '2026-11-01', state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' },
  { id: 'm2', title: 'v1.1', description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-02-01T00:00:00Z' },
];

export const ALICE: UserRef = { id: 'u1', username: 'alice' };
export const BASTIEN: UserRef = { id: 'u2', username: 'bastien' };
export const FLORIAN: UserRef = { id: 'u3', username: 'florian' };

export const TITLE_VERBS = ['Corriger', 'Ajouter', 'Documenter', 'Tester', 'Simplifier'];
export const TITLE_SUBJECTS = ['la page des pipelines', 'les webhooks', 'la recherche', 'le wiki', 'les jetons d’API', 'la vue kanban'];

export const LONG_TITLE = 'Quand on renomme une branche protégée depuis l’interface, les règles de protection restent attachées à l’ancien nom et la nouvelle branche accepte les pushs forcés';
export const LONG_USERNAME = 'Maximilien de La Tour d’Auvergne';

export const fakeToast = { show: () => {}, dismiss: () => {} };

export type StoryRole = 'owner' | 'reader' | 'contributor' | 'maintainer' | null;

export function fakeRepositoryContextService(role: StoryRole) {
  return { current: () => ({ repositoryId: 'repo-1', path: ['alice', 'ferrisgit'], role, ancestors: [], groupId: null }) };
}
