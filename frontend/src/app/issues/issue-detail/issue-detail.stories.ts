import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { IssueDetail } from './issue-detail';
import { Issue, IssueComment, IssuesService } from '../issues.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { MeService } from '../../shell/me.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserRef } from '../../shared/user-ref';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';
import {
  ALICE,
  BASTIEN,
  BUG,
  FLORIAN,
  LABELS,
  MILESTONES,
  UI,
  URGENT,
  fakeRepositoryContextService,
  fakeToast,
} from '../issue-story-fixtures';

const CAMILLE: UserRef = { id: 'u4', username: 'camille' };

function issue(fields: Partial<Issue> = {}): Issue {
  return {
    id: 'i42',
    number: 42,
    authorId: ALICE.id,
    assigneeId: fields.assignee?.id ?? null,
    title: 'Le bouton « Se connecter » ne répond plus sous Firefox',
    description:
      'Depuis la mise à jour de mardi, cliquer sur **Se connecter** ne fait plus rien sous Firefox 131.\n\n' +
      '### Pour reproduire\n\n1. Ouvrir la page de connexion\n2. Saisir un identifiant valide\n3. Cliquer sur le bouton\n\n' +
      'La console affiche `TypeError: form.requestSubmit is not a function`. Chrome et Safari ne sont pas touchés.',
    status: 'in_progress',
    kind: 'bug',
    parentIssueId: null,
    createdAt: daysAgo(2),
    closedAt: null,
    milestoneId: 'm1',
    labels: [BUG, URGENT],
    author: ALICE,
    assignee: BASTIEN,
    commentCount: 3,
    ...fields,
  };
}

let nextComment = 0;
const comment = (author: UserRef | null, body: string, createdAt: string): IssueComment => ({ id: `c${++nextComment}`, authorId: author?.id ?? 'gone', author, body, createdAt });

const COMMENTS: IssueComment[] = [
  comment(BASTIEN, 'Je reproduis aussi avec Firefox ESR. Ça ressemble au polyfill retiré dans la dernière version du bundle.', daysAgo(1)),
  comment(ALICE, 'Bien vu : le polyfill a sauté avec la mise à jour de `core-js`. Je peux préparer un correctif si personne ne l’a déjà pris.', hoursAgo(20)),
  comment(BASTIEN, 'Je m’en occupe, je rajoute un test e2e sous Firefox au passage.', hoursAgo(3)),
];

const LONG_DESCRIPTION = `## Contexte

Les pipelines échouent de façon intermittente sur l'étape \`cargo test\` quand deux jobs partagent le même runner. Le verrou sur le cache n'est pas relâché si le job est annulé.

## Journal

\`\`\`text
error: failed to acquire package cache lock: Resource temporarily unavailable (os error 11)
    at /home/runner/.cargo/registry/cache/index.crates.io-6f17d22bba15001f/serde-1.0.210.crate
error: could not compile \`ferrisgit-infrastructure\` (lib) due to 1 previous error; 3 warnings emitted
\`\`\`

## Pistes

| Option | Coût | Risque |
| --- | --- | --- |
| Un cache par job | Disque ×2 | Faible |
| Relâcher le verrou à l'annulation | Une journée | Moyen |

> À vérifier : le runner Kubernetes n'a peut-être pas le même comportement.

Chemin concerné : \`crates/ferrisgit-infrastructure/src/kubernetes/runner_pool/cache_lock_supervisor.rs\``;

const MANY_COMMENTS: IssueComment[] = Array.from({ length: 12 }, (_, index) => {
  const people = [BASTIEN, ALICE, FLORIAN, CAMILLE];
  const bodies = [
    'Je regarde ça cet après-midi.',
    'Le correctif du cache est en revue, voir la demande de fusion associée.',
    'Est-ce qu’on peut ajouter un test qui annule un job en plein `cargo test` ?\n\nSinon on risque de le casser à nouveau.',
    'Ok pour moi.',
  ];
  return comment(people[index % people.length], bodies[index % bodies.length], hoursAgo(60 - index * 5));
});

function fakeIssuesService(current: Issue, comments: IssueComment[], overrides: Partial<Record<keyof IssuesService, unknown>> = {}) {
  return {
    detail: () => of(current),
    listComments: () => of(comments),
    addComment: () => of(comments[0]),
    close: () => of({ ...current, status: 'done' as const, closedAt: minutesAgo(0) }),
    reopen: () => of({ ...current, status: 'todo' as const, closedAt: null }),
    assign: () => of({ ...current, assignee: FLORIAN, assigneeId: FLORIAN.id }),
    update: () => of(current),
    ...overrides,
  };
}

function withData(options: { issue?: Issue; comments?: IssueComment[]; role?: 'owner' | 'contributor' | 'reader'; issuesService?: unknown; labels?: Label[]; milestones?: Milestone[] } = {}) {
  const current = options.issue ?? issue();
  return moduleMetadata({
    providers: [
      { provide: IssuesService, useValue: options.issuesService ?? fakeIssuesService(current, options.comments ?? COMMENTS) },
      { provide: LabelsService, useValue: { listForRepository: () => of(options.labels ?? LABELS), setForIssue: () => of(current.labels) } },
      { provide: MilestonesService, useValue: { listForRepository: () => of(options.milestones ?? MILESTONES) } },
      { provide: RepositoryContextService, useValue: fakeRepositoryContextService(options.role ?? 'contributor') },
      { provide: MeService, useValue: { id: () => FLORIAN.id, username: () => FLORIAN.username, email: () => 'florian@example.com', isAdmin: () => false } },
      { provide: GbtToastService, useValue: fakeToast },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreX = (el: Element) => rect(el).left + rect(el).width / 2;
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

const RAIL_AXIS = 16;

/** Layout checks jsdom can't make, at any viewport: no horizontal overflow, aside beside the main column from 769px, avatars and composer marker centred on the rail. */
function assertPageLayout(canvas: HTMLElement): number {
  const layout = canvas.querySelector('gbt-page-layout');
  const main = canvas.querySelector('.gbt-page-layout__main');
  const aside = canvas.querySelector('.gbt-page-layout__aside');
  const discussion = canvas.querySelector('.issue-detail__discussion');
  if (!layout || !main || !aside || !discussion) throw new Error('page layout not rendered yet');

  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);

  if (rect(layout).width >= 769) {
    if (rect(aside).left < rect(main).right) throw new Error('the aside should sit beside the main column');
    if (Math.abs(rect(aside).top - rect(main).top) > 1) throw new Error('the aside should start level with the main column');
  } else if (rect(aside).top < rect(main).bottom) {
    throw new Error('the aside should stack under the main column');
  }

  const axis = rect(discussion).left + RAIL_AXIS;
  const cards = Array.from(canvas.querySelectorAll('.issue-timeline .issue-comment'));
  for (const card of cards) {
    const avatar = card.querySelector('.issue-comment__avatar')!;
    const header = card.querySelector('.issue-comment__header')!;
    if (Math.abs(centreX(avatar) - axis) > 0.5) throw new Error('avatar off the rail');
    if (Math.abs(centreY(avatar) - centreY(header)) > 1) throw new Error('avatar off its header');
    if (rect(card).right > rect(main).right + 0.5) throw new Error('card wider than the main column');
  }
  const marker = canvas.querySelector('.issue-detail__composer-marker')!;
  if (Math.abs(centreX(marker) - axis) > 0.5) throw new Error('composer marker off the rail');
  const composer = canvas.querySelector('.issue-detail__composer-card')!;
  const button = canvas.querySelector('.issue-detail__comment-button button')!;
  const padding = parseFloat(getComputedStyle(composer).paddingRight) + parseFloat(getComputedStyle(composer).borderRightWidth);
  if (Math.abs(rect(composer).right - padding - rect(button).right) > 1) throw new Error('"Commenter" not right-aligned in the composer');
  return cards.length;
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

const meta: Meta<IssueDetail> = {
  title: 'Issues/IssueDetail',
  component: IssueDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', number: 42 },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<IssueDetail>;

export const Open: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expect(await waitFor(() => assertPageLayout(context.canvasElement)), 'description + 3 comments').toBe(4);
    await expect(context.canvasElement.querySelector('.gbt-button--primary'), 'no primary button').toBeNull();
  },
};

export const Closed: Story = {
  decorators: [
    withData({
      issue: issue({ number: 43, title: 'Documenter la procédure de déploiement', status: 'done', kind: 'task', closedAt: hoursAgo(5), labels: [], milestoneId: null, assignee: ALICE }),
      comments: COMMENTS.slice(0, 1),
    }),
  ],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-page-header__actions')?.textContent).toContain('Rouvrir le ticket');
  },
};

export const ReadOnly: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-page-header__actions button')).toBeNull();
    await expect(context.canvasElement.querySelector('.gbt-select__trigger')).toBeNull();
    await expect(context.canvasElement.querySelector('.issue-detail__composer textarea')).not.toBeNull();
  },
};

export const LongDescription: Story = {
  decorators: [withData({ issue: issue({ number: 1287, title: 'Les pipelines échouent quand deux jobs partagent le cache cargo d’un même runner', kind: 'bug', description: LONG_DESCRIPTION }), comments: [] })],
  play: expectPageLayout,
};

export const ManyComments: Story = {
  decorators: [withData({ comments: MANY_COMMENTS })],
  play: async (context) => {
    await expect(await waitFor(() => assertPageLayout(context.canvasElement))).toBe(13);
    await expect(context.canvasElement.querySelectorAll('.issue-detail__people li').length).toBe(4);
  },
};

export const NoAssignee: Story = {
  decorators: [withData({ issue: issue({ assignee: null, assigneeId: null, status: 'todo', kind: 'feature', title: 'Proposer des modèles de tickets par dépôt', labels: [UI] }), comments: [] })],
  play: expectPageLayout,
};

export const UnknownAuthor: Story = {
  decorators: [withData({ issue: issue({ author: null, description: '' }), comments: [comment(null, 'Commentaire d’un compte supprimé.', daysAgo(1)), ...COMMENTS.slice(2)] })],
  play: expectPageLayout,
};

export const Loading: Story = {
  decorators: [withData({ issuesService: fakeIssuesService(issue(), [], { detail: () => NEVER, listComments: () => NEVER }) })],
};

export const LoadError: Story = {
  decorators: [withData({ issuesService: fakeIssuesService(issue(), [], { detail: () => throwError(() => new Error('500')) }) })],
};
