import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { MergeRequestDetail } from './merge-request-detail';
import { Comment, FileDiff, MergeAttemptResult, MergeRequestSummary, MergeRequestsService, ReviewSummary, TimelineItem, TimelineResponse } from '../merge-requests.service';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { withRouterAndIcons } from '../../repositories/repository-story-fixtures';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';
import { fakeToast } from '../../shared/layout/settings-story-helpers';
import { ALICE, BOB, commentFixture as comment, fakeRepositoryContext, label, mergeRequestFixture, milestone } from '../merge-request-fixtures';

const sampleLabels: Label[] = [label('l1', 'Backend', '#2563eb'), label('l2', 'sécurité', '#dc2626'), label('l3', 'interface', '#6366f1'), label('l4', 'documentation', '#0ea5e9')];
const sampleMilestones: Milestone[] = [milestone({ id: 'm1', title: 'v1.0' }), milestone({ id: 'm2', title: 'v1.1 — authentification unifiée', createdAt: '2026-02-01T00:00:00Z' })];

const sampleDiff: FileDiff[] = [
  {
    path: 'src/main.rs',
    change: 'modified',
    hunks: [
      {
        rows: [
          { oldLine: 1, oldContent: 'fn main() {\n', newLine: 1, newContent: 'fn main() {\n', kind: 'context' },
          { oldLine: 2, oldContent: '    println!("old");\n', newLine: null, newContent: null, kind: 'removed' },
          { oldLine: null, oldContent: null, newLine: 2, newContent: '    println!("new");\n', kind: 'added' },
          { oldLine: 3, oldContent: '}\n', newLine: 3, newContent: '}\n', kind: 'context' },
        ],
      },
    ],
  },
];

const reviewDiff: FileDiff[] = [
  {
    path: 'crates/ferrisgit-api/src/routes/auth/sso.rs',
    change: 'modified',
    hunks: [
      {
        rows: [
          { oldLine: 12, oldContent: 'pub async fn callback(State(state): State<AppState>, Query(params): Query<CallbackParams>) -> Result<Redirect, ApiError> {\n', newLine: 12, newContent: 'pub async fn callback(State(state): State<AppState>, Query(params): Query<CallbackParams>) -> Result<Redirect, ApiError> {\n', kind: 'context' },
          { oldLine: 13, oldContent: '    let token = state.oidc.exchange_code(&params.code).await?;\n', newLine: 13, newContent: '    let token = state.oidc.exchange_code(&params.code, &params.state).await.map_err(ApiError::unauthorized)?;\n', kind: 'modified' },
          { oldLine: 14, oldContent: '    let user = state.users.find_by_email(&token.email).await?;\n', newLine: null, newContent: null, kind: 'removed' },
          { oldLine: null, oldContent: null, newLine: 14, newContent: '    let user = state.users.find_or_provision(&token.email, &token.preferred_username).await?;\n', kind: 'added' },
          { oldLine: null, oldContent: null, newLine: 15, newContent: '    tracing::info!(user = %user.id, provider = %token.issuer, "signed in through SSO");\n', kind: 'added' },
          { oldLine: 15, oldContent: '    Ok(Redirect::to("/"))\n', newLine: 16, newContent: '    Ok(Redirect::to("/"))\n', kind: 'context' },
          { oldLine: 16, oldContent: '}\n', newLine: 17, newContent: '}\n', kind: 'context' },
        ],
      },
    ],
  },
  {
    path: 'frontend/src/app/login/sso-button/sso-button.ts',
    change: 'added',
    hunks: [
      {
        rows: [
          { oldLine: null, oldContent: null, newLine: 1, newContent: "import { Component, input } from '@angular/core';\n", kind: 'added' },
          { oldLine: null, oldContent: null, newLine: 2, newContent: '\n', kind: 'added' },
          { oldLine: null, oldContent: null, newLine: 3, newContent: "@Component({ selector: 'fg-sso-button', templateUrl: './sso-button.html' })\n", kind: 'added' },
          { oldLine: null, oldContent: null, newLine: 4, newContent: 'export class SsoButton {\n', kind: 'added' },
          { oldLine: null, oldContent: null, newLine: 5, newContent: '  provider = input.required<string>();\n', kind: 'added' },
          { oldLine: null, oldContent: null, newLine: 6, newContent: '}\n', kind: 'added' },
        ],
      },
    ],
  },
  {
    path: 'docs/auth/legacy-ldap.md',
    change: 'deleted',
    hunks: [
      {
        rows: [
          { oldLine: 1, oldContent: '# Connexion LDAP (obsolète)\n', newLine: null, newContent: null, kind: 'removed' },
          { oldLine: 2, oldContent: '\n', newLine: null, newContent: null, kind: 'removed' },
          { oldLine: 3, oldContent: 'Remplacée par la connexion SSO (OIDC).\n', newLine: null, newContent: null, kind: 'removed' },
        ],
      },
    ],
  },
  { path: 'frontend/src/assets/providers/keycloak.png', change: 'binary', hunks: [] },
];

const camille = { id: 'u5', username: 'camille.de-la-fontaine-desrosiers' };

const sampleComments: Comment[] = [
  comment({ id: 'c1', author: BOB, body: 'Bien vu, merci pour la revue rapide !', createdAt: hoursAgo(20) }),
  comment({ id: 'c2', author: ALICE, body: 'Pourquoi ce changement ici ?', createdAt: hoursAgo(6), filePath: 'src/main.rs', lineNumber: 2, side: 'new' }),
];

const blockedReviewSummary: ReviewSummary = {
  reviews: [{ userId: 'u3', username: 'carol', decision: 'approved', stale: false, createdAt: hoursAgo(20) }],
  requiredApprovals: 2,
  liveApprovalCount: 1,
  blocked: true,
};

const inlineThread: TimelineItem = {
  type: 'thread',
  id: 'c2',
  createdAt: hoursAgo(6),
  filePath: 'src/main.rs',
  lineNumber: 2,
  endLine: null,
  side: 'new',
  outdated: false,
  resolved: false,
  resolvedBy: null,
  resolvedAt: null,
  excerpt: [
    { line: 1, kind: 'context', content: 'fn main() {\n' },
    { line: 2, kind: 'added', content: '    println!("new");\n' },
  ],
  root: sampleComments[1],
  replies: [],
};

const mixedTimeline: TimelineResponse = {
  author: ALICE,
  items: [
    { type: 'event', id: 'e1', createdAt: daysAgo(3), actor: ALICE, kind: 'labels_changed', payload: { added: [{ id: 'l1', name: 'Backend', color: '#2563eb' }], removed: [] } },
    { type: 'comment', id: 'c1', createdAt: hoursAgo(20), author: BOB, body: 'Bien vu, merci pour la revue rapide !' },
    inlineThread,
    { type: 'event', id: 'e2', createdAt: hoursAgo(2), actor: ALICE, kind: 'commits_pushed', payload: { fromSha: '1a2b3c4d5e6f', toSha: '9f8e7d6c5b4a' } },
    { type: 'event', id: 'e3', createdAt: minutesAgo(35), actor: { id: 'u3', username: 'carol' }, kind: 'review_submitted', payload: { decision: 'approved' } },
  ],
};

const mergedTimeline: TimelineResponse = {
  author: ALICE,
  items: [
    { type: 'comment', id: 'c1', createdAt: hoursAgo(20), author: BOB, body: 'Tout est bon pour moi.' },
    { type: 'event', id: 'e1', createdAt: hoursAgo(3), actor: BOB, kind: 'review_submitted', payload: { decision: 'approved' } },
    { type: 'event', id: 'e2', createdAt: minutesAgo(12), actor: ALICE, kind: 'merged', payload: { mergeCommitSha: 'c0ffee1234567' } },
  ],
};

const changesRequestedTimeline: TimelineResponse = {
  author: ALICE,
  items: [inlineThread, { type: 'event', id: 'e1', createdAt: hoursAgo(4), actor: BOB, kind: 'review_submitted', payload: { decision: 'changes_requested' } }],
};

const changesRequestedReviewSummary: ReviewSummary = {
  reviews: [
    { userId: 'u3', username: 'carol', decision: 'approved', stale: true, createdAt: hoursAgo(9) },
    { userId: 'u2', username: 'bob', decision: 'changes_requested', stale: false, createdAt: hoursAgo(4) },
  ],
  requiredApprovals: 1,
  liveApprovalCount: 0,
  blocked: true,
};

const satisfiedReviewSummary: ReviewSummary = {
  reviews: [
    { userId: 'u3', username: 'carol', decision: 'approved', stale: false, createdAt: hoursAgo(9) },
    { userId: 'u2', username: 'bob', decision: 'approved', stale: false, createdAt: hoursAgo(1) },
  ],
  requiredApprovals: 2,
  liveApprovalCount: 2,
  blocked: false,
};

const noReviews: ReviewSummary = { reviews: [], requiredApprovals: 0, liveApprovalCount: 0, blocked: false };

const openMergeRequest = mergeRequestFixture({
  id: 'mr-1',
  title: 'Ajoute la connexion via SSO',
  sourceBranch: 'feature/sso-login',
  description: 'Implémente le flux OAuth pour la connexion.',
  createdAt: daysAgo(3),
  milestoneId: 'm1',
  labels: [sampleLabels[0]],
  commentCount: 2,
});

const mergedMergeRequest: MergeRequestSummary = {
  ...openMergeRequest,
  id: 'mr-3',
  title: 'Corrige le calcul des taxes',
  status: 'merged',
  mergeCommitSha: 'c0ffee1234567',
  closedAt: minutesAgo(12),
};

const closedMergeRequest: MergeRequestSummary = {
  ...openMergeRequest,
  id: 'mr-2',
  title: 'Ancienne demande de fusion abandonnée',
  status: 'closed',
  closedAt: minutesAgo(8),
  labels: [],
  milestoneId: null,
};

interface DataOptions {
  diffs?: FileDiff[];
  comments?: Comment[];
  reviewSummary?: ReviewSummary;
  timeline?: TimelineResponse;
  merge?: () => Observable<MergeAttemptResult>;
  detail?: () => Observable<MergeRequestSummary>;
}

function fakeMergeRequestsService(mr: MergeRequestSummary, options: DataOptions = {}) {
  const diffs = options.diffs ?? sampleDiff;
  const comments = options.comments ?? sampleComments;
  const reviewSummary = options.reviewSummary ?? blockedReviewSummary;
  const timeline = options.timeline ?? mixedTimeline;
  return {
    detail: options.detail ?? (() => of(mr)),
    diff: () => of(diffs),
    listComments: () => of(comments),
    listReviews: () => of(reviewSummary),
    timeline: () => of(timeline),
    addComment: () => of(comments[0]),
    resolveComment: () => of(undefined),
    unresolveComment: () => of(undefined),
    applySuggestion: () => of(comments[0]),
    submitReview: () => of(reviewSummary.reviews[0]),
    merge: options.merge ?? (() => of({ ...mr, outcome: 'merged' as const })),
    close: () => of(undefined),
    update: () => of(mr),
  };
}

function withData(mr: MergeRequestSummary, role: 'owner' | 'reader' | 'contributor', options: DataOptions = {}, labels: Label[] = sampleLabels, milestones: Milestone[] = sampleMilestones) {
  return moduleMetadata({
    providers: [
      { provide: MergeRequestsService, useValue: fakeMergeRequestsService(mr, options) },
      { provide: LabelsService, useValue: { listForRepository: () => of(labels), setForMergeRequest: () => of(labels) } },
      { provide: MilestonesService, useValue: { listForRepository: () => of(milestones) } },
      { provide: RepositoryContextService, useValue: fakeRepositoryContext(role, ['acme', 'widget']) },
      { provide: GbtToastService, useValue: fakeToast },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();

/** Layout checks jsdom can't make: no horizontal overflow, aside beside the main column from 769px (none on Modifications), header actions centred on the title's first line. */
function assertPageLayout(canvas: HTMLElement, expectAside: boolean): void {
  const layout = canvas.querySelector('gbt-page-layout');
  const main = canvas.querySelector('.gbt-page-layout__main');
  const aside = canvas.querySelector('.gbt-page-layout__aside');
  if (!layout || !main || !aside) throw new Error('page layout not rendered yet');

  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);

  for (const card of Array.from(main.querySelectorAll('.mr-diff__file, gbt-alert, fg-merge-request-timeline'))) {
    if (rect(card).width > 0 && rect(card).right > rect(main).right + 0.5) throw new Error(`${card.className || card.tagName} wider than the main column`);
  }

  if (!expectAside) {
    if (aside.children.length > 0) throw new Error('the aside should be hidden on the Modifications tab');
    if (Math.abs(rect(main).width - rect(layout).width) > 1) throw new Error('the diff should take the whole layout width');
    return;
  }
  if (aside.children.length === 0) throw new Error('aside not rendered yet');

  if (rect(layout).width >= 769) {
    if (rect(aside).left < rect(main).right) throw new Error('the aside should sit beside the main column');
    if (Math.abs(rect(aside).top - rect(main).top) > 1) throw new Error('the aside should start level with the main column');
  } else if (rect(aside).top < rect(main).bottom) {
    throw new Error('the aside should stack under the main column');
  }
  for (const node of Array.from(aside.querySelectorAll('gbt-panel, .mr-approvals li, .mr-approvals__actions gbt-button, gbt-select, gbt-user-chip'))) {
    if (rect(node).right > rect(aside).right + 0.5) throw new Error(`${node.tagName.toLowerCase()} wider than the aside`);
  }

  const actions = canvas.querySelector('.gbt-page-header__actions');
  const title = canvas.querySelector('.gbt-page-header__title');
  if (actions && title && rect(actions).top < rect(title).bottom && rect(actions).left > rect(title).left) {
    const titleLine = parseFloat(getComputedStyle(title).lineHeight);
    const actionsCentre = rect(actions).top + rect(actions).height / 2;
    const titleCentre = rect(title).top + titleLine / 2;
    if (Math.abs(actionsCentre - titleCentre) > 6) throw new Error('header actions off the title line');
  }
}

const expectPageLayout = (expectAside = true) =>
  async function play({ canvasElement }: { canvasElement: HTMLElement }) {
    await waitFor(() => assertPageLayout(canvasElement, expectAside), { timeout: 3000 });
  };

function button(canvas: HTMLElement, selector: string, text: string): HTMLButtonElement {
  const found = Array.from(canvas.querySelectorAll<HTMLButtonElement>(selector)).find((b) => b.textContent?.trim() === text);
  if (!found) throw new Error(`no "${text}" button`);
  return found;
}

const meta: Meta<MergeRequestDetail> = {
  title: 'MergeRequests/MergeRequestDetail',
  component: MergeRequestDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: {
    repositoryId: 'repo-1',
    mergeRequestId: 'mr-1',
  },
  decorators: [withRouterAndIcons, inShellContentArea],
};

export default meta;
type Story = StoryObj<MergeRequestDetail>;

export const Populated: Story = {
  decorators: [withData(openMergeRequest, 'owner')],
  play: async (context) => {
    await expectPageLayout()(context);
    const primaries = context.canvasElement.querySelectorAll('.gbt-button--primary');
    await expect(primaries.length, 'one primary button').toBe(1);
    await expect(button(context.canvasElement, '.gbt-page-header__actions button', 'Fusionner').disabled).toBe(true);
    await expect(context.canvasElement.querySelectorAll('.mr-detail__people li').length, 'alice, BOB, carol').toBe(3);
  },
};

export const ChangesRequested: Story = {
  decorators: [withData(openMergeRequest, 'owner', { reviewSummary: changesRequestedReviewSummary, timeline: changesRequestedTimeline })],
  play: expectPageLayout(),
};

export const Approved: Story = {
  decorators: [withData(openMergeRequest, 'owner', { reviewSummary: satisfiedReviewSummary })],
  play: async (context) => {
    await expectPageLayout()(context);
    await expect(button(context.canvasElement, '.gbt-page-header__actions button', 'Fusionner').disabled).toBe(false);
    await expect(context.canvasElement.querySelector('.mr-approvals gbt-alert')).toBeNull();
  },
};

export const ConflictAlert: Story = {
  decorators: [withData(openMergeRequest, 'owner', { reviewSummary: satisfiedReviewSummary, merge: () => of({ outcome: 'conflicting' as const }) })],
  play: async (context) => {
    await waitFor(() => button(context.canvasElement, '.gbt-page-header__actions button', 'Fusionner').click());
    await waitFor(() => expect(context.canvasElement.querySelector('.mr-detail__conflict')).not.toBeNull());
    await expectPageLayout()(context);
  },
};

export const Merged: Story = {
  args: { mergeRequestId: 'mr-3' },
  decorators: [withData(mergedMergeRequest, 'owner', { comments: [], timeline: mergedTimeline, reviewSummary: noReviews })],
  play: async (context) => {
    await expectPageLayout()(context);
    await expect(context.canvasElement.querySelector('.gbt-page-header__actions button')).toBeNull();
    await expect(context.canvasElement.querySelector('.mr-approvals')).toBeNull();
  },
};

export const Closed: Story = {
  args: { mergeRequestId: 'mr-2' },
  decorators: [
    withData(
      closedMergeRequest,
      'owner',
      { comments: [], timeline: { author: ALICE, items: [{ type: 'event', id: 'e1', createdAt: minutesAgo(8), actor: ALICE, kind: 'closed', payload: {} }] }, reviewSummary: noReviews },
      [],
      [],
    ),
  ],
  play: expectPageLayout(),
};

export const ReadOnly: Story = {
  decorators: [withData({ ...openMergeRequest, labels: sampleLabels.slice(0, 3) }, 'reader')],
  play: async (context) => {
    await expectPageLayout()(context);
    await expect(context.canvasElement.querySelector('.gbt-page-header__actions button')).toBeNull();
    await expect(context.canvasElement.querySelector('.gbt-select__trigger')).toBeNull();
    await expect(context.canvasElement.querySelector('.mr-approvals__actions')).toBeNull();
  },
};

/** A Contributor can review and close but not merge, so the header keeps only Fermer. */
export const ContributorCannotMerge: Story = {
  decorators: [withData(openMergeRequest, 'contributor', { reviewSummary: satisfiedReviewSummary })],
  play: async (context) => {
    await expectPageLayout()(context);
    await expect(button(context.canvasElement, '.gbt-page-header__actions button', 'Fermer')).toBeTruthy();
    await expect(
      Array.from(context.canvasElement.querySelectorAll('.gbt-page-header__actions button')).some((b) => b.textContent?.trim() === 'Fusionner'),
    ).toBe(false);
  },
};

export const LongContent: Story = {
  decorators: [
    withData(
      {
        ...openMergeRequest,
        title: 'Remplace l’ancien connecteur LDAP par une connexion SSO (OIDC) avec provisionnement automatique des comptes et des groupes',
        sourceBranch: 'feature/remplacement-ldap-par-sso-oidc-avec-provisionnement',
        targetBranch: 'release/2026.10-authentification',
        labels: sampleLabels,
        milestoneId: 'm2',
        author: camille,
      },
      'owner',
      {
        reviewSummary: {
          ...changesRequestedReviewSummary,
          reviews: [...changesRequestedReviewSummary.reviews, { userId: camille.id, username: camille.username, decision: 'approved', stale: false, createdAt: hoursAgo(1) }],
        },
        timeline: { ...mixedTimeline, author: camille },
      },
    ),
  ],
  play: expectPageLayout(),
};

export const ChangesTab: Story = {
  decorators: [withData(openMergeRequest, 'owner', { diffs: reviewDiff, comments: [] })],
  play: async (context) => {
    await waitFor(() => button(context.canvasElement, '[role="tab"]', 'Modifications').click());
    await expectPageLayout(false)(context);
    await expect(context.canvasElement.querySelectorAll('.mr-diff__file').length).toBe(4);
  },
};

/** The review bar over the diff fits the main column, and when actions share its row both are centred. */
async function expectReviewBar(canvas: HTMLElement, status: string) {
  await waitFor(() => button(canvas, '[role="tab"]', 'Modifications').click());
  await expectPageLayout(false)({ canvasElement: canvas });
  await waitFor(() => {
    const bar = canvas.querySelector('gbt-alert[data-review-bar]');
    const main = canvas.querySelector('.gbt-page-layout__main');
    if (!bar || !main) throw new Error('review bar not rendered yet');
    if (rect(bar).right > rect(main).right + 0.5) throw new Error('review bar wider than the main column');
    const text = canvas.querySelector('[data-review-status]');
    const actions = canvas.querySelector('gbt-button[alert-actions]');
    if (text && actions && rect(actions).top < rect(text).bottom) {
      const centre = (el: Element) => rect(el).top + rect(el).height / 2;
      if (Math.abs(centre(text) - centre(actions)) > 1) throw new Error('review bar status and actions off one line');
    }
  });
  await expect(canvas.querySelector('[data-review-status]')?.textContent?.trim()).toBe(status);
  await expect(button(canvas, 'gbt-alert[data-review-bar] button', 'Approuver')).toBeTruthy();
  await expect(button(canvas, 'gbt-alert[data-review-bar] button', 'Demander des changements')).toBeTruthy();
  await expect(canvas.querySelectorAll('.gbt-button--primary').length, 'Fusionner stays the only primary').toBe(1);
}

export const DiffTabWithReviewBar: Story = {
  decorators: [withData(openMergeRequest, 'owner', { diffs: reviewDiff, comments: [] })],
  play: ({ canvasElement }) => expectReviewBar(canvasElement, '1/2 approbations requises.'),
};

export const DiffTabChangesRequested: Story = {
  decorators: [withData(openMergeRequest, 'owner', { diffs: reviewDiff, comments: [], reviewSummary: changesRequestedReviewSummary, timeline: changesRequestedTimeline })],
  play: ({ canvasElement }) => expectReviewBar(canvasElement, 'Des changements ont été demandés.'),
};

export const Loading: Story = {
  decorators: [withData(openMergeRequest, 'owner', { detail: () => NEVER })],
};

export const LoadError: Story = {
  decorators: [withData(openMergeRequest, 'owner', { detail: () => throwError(() => new Error('500')) })],
};
