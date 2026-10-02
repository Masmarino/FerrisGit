import { componentWrapperDecorator, moduleMetadata, type Meta, type StoryObj } from '@storybook/angular-vite';
import { Card, CardHeader } from '@masmarino/gabarit';
import { expect, userEvent, within } from 'storybook/test';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';
import { FileDiffView } from './file-diff-view';
import { Comment, FileDiff, SplitDiffRow } from '../merge-requests.service';
import { commentFixture } from '../merge-request-fixtures';

type RowKind = SplitDiffRow['kind'];

function row(kind: RowKind, oldLine: number | null, newLine: number | null, oldContent: string | null, newContent: string | null): SplitDiffRow {
  return { kind, oldLine, newLine, oldContent, newContent };
}

const context = (oldLine: number, newLine: number, content: string) => row('context', oldLine, newLine, content, content);
const removed = (oldLine: number, content: string) => row('removed', oldLine, null, content, null);
const added = (newLine: number, content: string) => row('added', null, newLine, null, content);
const modified = (oldLine: number, newLine: number, oldContent: string, newContent: string) => row('modified', oldLine, newLine, oldContent, newContent);

const modifiedFile: FileDiff = {
  path: 'src/auth/session.rs',
  change: 'modified',
  hunks: [
    {
      rows: [
        context(10, 10, 'use crate::config::Settings;\n'),
        added(11, 'use crate::auth::sso::SsoProvider;\n'),
        context(11, 12, 'use crate::error::AppError;\n'),
        context(12, 13, '\n'),
        context(13, 14, 'pub struct Session {\n'),
        context(14, 15, '    pub user_id: Uuid,\n'),
        modified(15, 16, '    pub expires_at: i64,\n', '    pub expires_at: DateTime<Utc>,\n'),
        context(16, 17, '}\n'),
      ],
    },
    {
      rows: [
        context(41, 42, 'impl Session {\n'),
        context(42, 43, '    pub fn is_valid(&self) -> bool {\n'),
        modified(43, 44, '        self.expires_at > now_timestamp()\n', '        self.expires_at > Utc::now()\n'),
        context(44, 45, '    }\n'),
        context(45, 46, '\n'),
        removed(46, '    pub fn refresh(&mut self) {\n'),
        removed(47, '        self.expires_at = now_timestamp() + 3600;\n'),
        removed(48, '    }\n'),
        added(47, '    pub fn refresh(&mut self, ttl: Duration) {\n'),
        added(48, '        self.expires_at = Utc::now() + ttl;\n'),
        added(49, '    }\n'),
        context(49, 50, '}\n'),
      ],
    },
    {
      rows: [
        context(88, 89, 'fn now_timestamp() -> i64 {\n'),
        removed(89, '    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64\n'),
        added(90, '    Utc::now().timestamp()\n'),
        context(90, 91, '}\n'),
      ],
    },
  ],
};

const addedFile: FileDiff = {
  path: 'src/auth/sso.rs',
  change: 'added',
  hunks: [
    {
      rows: [
        added(1, '//! Connexion via un fournisseur OAuth externe.\n'),
        added(2, 'use crate::error::AppError;\n'),
        added(3, '\n'),
        added(4, 'pub struct SsoProvider {\n'),
        added(5, '    pub issuer: String,\n'),
        added(6, '    pub client_id: String,\n'),
        added(7, '}\n'),
        added(8, '\n'),
        added(9, 'impl SsoProvider {\n'),
        added(10, '    pub fn authorize_url(&self) -> Result<String, AppError> {\n'),
        added(11, '        Ok(format!("{}/authorize?client_id={}", self.issuer, self.client_id))\n'),
        added(12, '    }\n'),
        added(13, '}\n'),
      ],
    },
  ],
};

const deletedFile: FileDiff = {
  path: 'src/legacy/basic_auth.rs',
  change: 'deleted',
  hunks: [
    {
      rows: [
        removed(1, '// Authentification HTTP Basic, remplacée par les jetons API.\n'),
        removed(2, 'pub fn check_basic(header: &str) -> bool {\n'),
        removed(3, '    header.starts_with("Basic ")\n'),
        removed(4, '}\n'),
      ],
    },
  ],
};

const binaryFile: FileDiff = { path: 'docs/images/architecture.png', change: 'binary', hunks: [] };

const longLinesFile: FileDiff = {
  path: 'crates/ferrisgit-api/src/routes/merge_requests.rs',
  change: 'modified',
  hunks: [
    {
      rows: [
        context(120, 120, 'pub async fn list_merge_requests(\n'),
        modified(
          121,
          121,
          '    State(state): State<AppState>, Path((owner, name)): Path<(String, String)>, Query(params): Query<ListMergeRequestsParams>, Extension(current_user): Extension<Option<CurrentUser>>,\n',
          '    State(state): State<AppState>,\n',
        ),
        added(122, '    Path((owner, name)): Path<(String, String)>,\n'),
        added(123, '    Query(params): Query<ListMergeRequestsParams>,\n'),
        context(122, 124, ') -> Result<Json<Vec<MergeRequestResponse>>, ApiError> {\n'),
        removed(123, '    let merge_requests = state.merge_requests.list_for_repository(repository.id, params.status.as_deref(), params.page.unwrap_or(1), params.per_page.unwrap_or(20)).await?;\n'),
        added(125, '    let page = Page::new(params.page, params.per_page);\n'),
        added(126, '    let merge_requests = state.merge_requests.list_for_repository(repository.id, params.status.as_deref(), page).await.map_err(|error| ApiError::internal(format!("impossible de lister les demandes de fusion : {error}")))?;\n'),
        context(124, 127, '    Ok(Json(merge_requests.into_iter().map(Into::into).collect()))\n'),
      ],
    },
    {
      rows: [
        context(9870, 9873, 'fn url_for(owner: &str, name: &str, number: i64) -> String {\n'),
        modified(9871, 9874, '    format!("/{owner}/{name}/-/merge_requests/{number}")\n', '    format!("/{owner}/{name}/-/merge_requests/{number}?tab=modifications&view=split&whitespace=ignore&expand=all")\n'),
        context(9872, 9875, '}\n'),
      ],
    },
  ],
};

const manyHunksFile: FileDiff = {
  path: 'frontend/src/app/merge-requests/merge-requests.service.ts',
  change: 'modified',
  // Even hunks add a line, odd ones remove one, so the new side is one line ahead after an even hunk.
  hunks: Array.from({ length: 6 }, (_, index) => {
    const start = 12 + index * 40;
    const shift = index % 2;
    const change = index % 2 === 0 ? added(start + 2, `  readonly extra${index} = true;\n`) : removed(start + 2, `  readonly legacy${index} = false;\n`);
    return {
      rows: [
        context(start, start + shift, `  // section ${index + 1}\n`),
        modified(start + 1, start + 1 + shift, `  readonly limit${index} = ${index * 10};\n`, `  readonly limit${index} = ${index * 10 + 5}; // relevé\n`),
        change,
        context(start + 2 + shift, start + 3, '\n'),
      ],
    };
  }),
};

function makeComment(overrides: Partial<Comment> = {}): Comment {
  return commentFixture({
    id: 'c1',
    authorId: 'u2',
    author: null,
    body: 'Commentaire',
    createdAt: '2026-09-23T10:00:00Z',
    filePath: 'src/auth/session.rs',
    lineNumber: 16,
    side: 'new',
    ...overrides,
  });
}

const inlineComments: Comment[] = [
  makeComment({ id: 'c1', body: 'Bonne idée de passer à DateTime<Utc>, cela évite les conversions en secondes partout.', lineNumber: 16 }),
  makeComment({ id: 'c2', authorId: 'u3', body: 'Cette suppression casse le rafraîchissement automatique côté client, non ?', lineNumber: 47, side: 'old', createdAt: '2026-09-23T10:20:00Z' }),
];

const threadComments: Comment[] = [
  makeComment({ id: 't1', body: 'Pourquoi ne pas garder la durée de vie fixe à une heure ?', lineNumber: 48, createdAt: '2026-09-23T09:00:00Z' }),
  makeComment({ id: 't1-r1', authorId: 'u1', replyToId: 't1', body: 'Elle doit désormais être configurable depuis les réglages admin.', createdAt: '2026-09-23T09:12:00Z', lineNumber: null, side: null, filePath: null }),
  makeComment({ id: 't1-r2', authorId: 'u2', replyToId: 't1', body: 'D’accord, merci pour la précision.', createdAt: '2026-09-23T09:30:00Z', lineNumber: null, side: null, filePath: null }),
  makeComment({ id: 't2', authorId: 'u3', body: 'Petit doute sur le fuseau horaire utilisé ici.', lineNumber: 44, resolved: true, createdAt: '2026-09-22T15:00:00Z' }),
  makeComment({ id: 't3', authorId: 'u3', body: 'Cette ligne a changé depuis mon dernier passage.', lineNumber: 90, outdated: true, createdAt: '2026-09-21T11:00:00Z' }),
];

const suggestionComments: Comment[] = [
  makeComment({
    id: 's1',
    body: 'On peut simplifier avec `Utc::now().timestamp()` directement.',
    lineNumber: 90,
    suggestedContent: '    Utc::now().timestamp_millis() / 1000\n',
  }),
  makeComment({
    id: 's2',
    authorId: 'u3',
    body: 'Nom plus explicite pour le champ.',
    lineNumber: 16,
    suggestedContent: '    pub expires_at: DateTime<Utc>, // heure d’expiration UTC\n',
    appliedAt: '2026-09-23T14:00:00Z',
    appliedCommitSha: 'a1b2c3d',
    createdAt: '2026-09-23T10:40:00Z',
  }),
];

// The file card as the merge request page draws it; cardWidth pins it to a phone's content width.
const inFileCard = componentWrapperDecorator(
  (story) => `<article [style.max-width]="cardWidth" style="margin-inline: auto; min-width: 0;">
    <gbt-card variant="outlined" [flush]="true">
      <div card-header style="color: var(--text-primary); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.8125rem; font-weight: 600;">{{ file.path }}</div>
      ${story}
    </gbt-card>
  </article>`,
  ({ parameters }) => ({ cardWidth: parameters['cardWidth'] ?? null }),
);

/** Layout checks jsdom can't make: hunks share columns, no code cell overflows, gutters keep their padding, no sideways page scroll. */
async function expectDiffLayout({ canvasElement }: { canvasElement: HTMLElement }): Promise<void> {
  const table = canvasElement.querySelector<HTMLTableElement>('.file-diff-view__table');
  await expect(table, 'the diff table is rendered').not.toBeNull();
  const page = canvasElement.ownerDocument.documentElement;
  await expect(page.scrollWidth, 'the page does not scroll sideways').toBeLessThanOrEqual(page.clientWidth);

  const widths = (row: Element) => Array.from(row.children, (cell) => Math.round(cell.getBoundingClientRect().width));
  const firstRows = Array.from(table!.tBodies, (body) => body.querySelector('.file-diff-view__row')).filter((row) => row !== null);
  const columns = widths(firstRows[0]!);
  for (const row of firstRows) {
    await expect(widths(row!), 'every hunk shares the same columns').toEqual(columns);
  }
  if (columns.length === 4 && getComputedStyle(table!).display === 'table') {
    await expect(Math.abs(columns[1] - columns[3]), 'the two code halves are equally wide').toBeLessThanOrEqual(1);
  }

  const tableRight = table!.getBoundingClientRect().right;
  const overflowing = Array.from(table!.querySelectorAll<HTMLElement>('.file-diff-view__content')).filter(
    (cell) => cell.scrollWidth > cell.clientWidth + 1 || cell.getBoundingClientRect().right > tableRight + 0.5,
  );
  await expect(overflowing.length, 'every line stays inside its half').toBe(0);

  const gutter = table!.querySelector('.file-diff-view__gutter')!;
  await expect(parseFloat(getComputedStyle(gutter).paddingLeft), 'line numbers keep off the card edge').toBeGreaterThanOrEqual(4);
}

async function expectSingleSide(context: { canvasElement: HTMLElement }): Promise<void> {
  await expectDiffLayout(context);
  await expect(context.canvasElement.querySelectorAll('.file-diff-view__table col').length, 'one gutter and one code column').toBe(2);
  await expect(context.canvasElement.querySelectorAll('.file-diff-view__content--empty').length, 'no empty half').toBe(0);
}

const meta: Meta<FileDiffView> = {
  title: 'MergeRequests/FileDiffView',
  component: FileDiffView,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [moduleMetadata({ imports: [Card, CardHeader] }), inFileCard, inShellContentArea],
  args: {
    file: modifiedFile,
    comments: [],
    canWrite: false,
  },
};

export default meta;
type Story = StoryObj<FileDiffView>;

export const Modified: Story = {
  play: expectDiffLayout,
};

export const Added: Story = {
  args: { file: addedFile },
  play: expectSingleSide,
};

export const Deleted: Story = {
  args: { file: deletedFile },
  play: expectSingleSide,
};

export const Binary: Story = {
  args: { file: binaryFile },
};

export const LongLines: Story = {
  args: { file: longLinesFile },
  play: expectDiffLayout,
};

export const ManyHunks: Story = {
  args: { file: manyHunksFile },
  play: expectDiffLayout,
};

/** At phone width the split view reads as a unified diff. */
export const LongLinesPhoneWidth: Story = {
  args: { file: longLinesFile },
  parameters: { cardWidth: '343px' },
  play: expectDiffLayout,
};

export const WithInlineComments: Story = {
  args: { comments: inlineComments },
};

export const WithCommentThreads: Story = {
  args: { comments: threadComments, canWrite: true },
};

export const WithSuggestions: Story = {
  args: { comments: suggestionComments, canWrite: true },
};

export const WithSuggestionsReadOnly: Story = {
  args: { comments: suggestionComments, canWrite: false },
};

export const ComposingComment: Story = {
  args: { canWrite: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Commenter la ligne 16 (nouvelle version)' }));
  },
};

export const ComposingSuggestion: Story = {
  args: { canWrite: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Commenter la ligne 44 (nouvelle version)' }));
    await userEvent.click(await canvas.findByRole('button', { name: 'Proposer un remplacement' }));
  },
};
