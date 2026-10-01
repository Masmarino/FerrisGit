import type { Meta, StoryObj } from '@storybook/angular-vite';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { expect, userEvent } from 'storybook/test';
import { RepositoryBlobView } from './repository-blob-view';
import { BlobContent, RepositoriesService, TreeEntry } from '../repositories.service';
import { atPhoneWidth, inShellContentArea } from '../../shared/layout/page-story-helpers';
import { fakeRepositoriesService, FOLDER_ENTRIES, POPULATED, README, REPO, ROOT_ENTRIES, withRepository, withRouterAndIcons } from '../repository-story-fixtures';

const dir = (name: string): TreeEntry => ({ name, isDir: true, lastCommit: null });
const file = (name: string): TreeEntry => ({ name, isDir: false, lastCommit: null });

const LEVELS: Record<string, TreeEntry[]> = {
  '': ROOT_ENTRIES,
  crates: FOLDER_ENTRIES,
  'crates/ferrisgit-api': [dir('src'), dir('tests'), file('Cargo.toml')],
  'crates/ferrisgit-api/src': [dir('routes'), file('error.rs'), file('lib.rs'), file('main.rs'), file('state.rs')],
  'crates/ferrisgit-api/src/routes': [file('issues.rs'), file('merge_requests.rs'), file('mod.rs'), file('pipelines.rs'), file('repositories.rs')],
  frontend: [dir('src'), file('angular.json'), file('package.json'), file('tsconfig.json')],
  'frontend/src': [dir('app'), dir('assets'), dir('styles'), file('index.html'), file('main.ts'), file('styles.scss')],
  'frontend/src/assets': [file('favicon.ico'), file('logo.png')],
  'frontend/src/app': [dir('merge-requests'), dir('repositories'), dir('shared'), dir('shell'), file('app.config.ts'), file('app.routes.ts'), file('app.ts')],
  'frontend/src/app/shared': [dir('code-view'), dir('layout'), dir('markdown-view'), file('register-icons.ts'), file('relative-time.pipe.ts')],
  'frontend/src/app/shared/code-view': [file('code-view.scss'), file('code-view.spec.ts'), file('code-view.stories.ts'), file('code-view.ts')],
  scripts: [file('dev.sh'), file('dump.sql'), file('release.sh')],
};

const OTHER_FOLDER: TreeEntry[] = [dir('fixtures'), file('mod.rs'), file('README.md')];

const MAIN_TS = `import { bootstrapApplication } from '@angular/platform-browser';
import { appConfig } from './app/app.config';
import { App } from './app/app';

// Zoneless: change detection follows signals, no zone.js in the bundle.
bootstrapApplication(App, appConfig).catch((err) => console.error(err));
`;

const CODE_VIEW_TS = `import { Component, computed, inject, input } from '@angular/core';
import { DomSanitizer, SafeHtml } from '@angular/platform-browser';
import hljs from 'highlight.js/lib/core';
import DOMPurify from 'dompurify';

/** Number of lines a file shows: a trailing newline ends the last line. */
export function lineCount(content: string): number {
  if (content === '') return 1;
  const breaks = content.split('\\n').length - 1;
  return content.endsWith('\\n') ? breaks : breaks + 1;
}

@Component({
  selector: 'fg-code-view',
  standalone: true,
  templateUrl: './code-view.html',
  styleUrl: './code-view.scss',
})
export class CodeView {
  content = input.required<string>();
  fileName = input.required<string>();
  lineNumbers = input(false);

  private sanitizer = inject(DomSanitizer);

  protected renderedHtml = computed<SafeHtml>(() => {
    const result = hljs.highlightAuto(this.content());
    return this.sanitizer.bypassSecurityTrustHtml(DOMPurify.sanitize(result.value));
  });
}
`;

function longRustFile(): string {
  const header = `//! Routes HTTP des dépôts : lecture de l'arbre, des fichiers, des commits et des contributeurs, protocole Git smart HTTP exclu (voir \`git_http.rs\`).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RefQuery {
    /// Branche, tag ou sha ; \`HEAD\` quand il est absent.
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}
`;
  const handlers = ['tree', 'blob', 'readme', 'commits', 'contributors', 'languages', 'branches', 'tags'].map(
    (name) => `
/// \`GET /api/repositories/by-id/{id}/${name}\` — réponse JSON, 404 quand la référence ou le chemin n'existe pas.
pub async fn get_${name}(State(state): State<AppState>, Path(id): Path<uuid::Uuid>, Query(query): Query<RefQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let repository = state.repositories.find_by_id(id).await?.ok_or(ApiError::NotFound)?;
    let reference = query.reference.as_deref().unwrap_or("HEAD");
    let reader = state.git.open(&repository.storage_path)?;
    let value = reader.${name}_at(reference).map_err(|err| ApiError::from_git(err, StatusCode::NOT_FOUND))?;
    tracing::debug!(repository = %repository.id, reference, "${name} lu depuis le dépôt nu sans cloner l'arbre de travail, en réutilisant le cache d'objets partagé entre les requêtes");
    Ok(Json(serde_json::to_value(value)?))
}
`,
  );
  return header + handlers.join('');
}

interface BlobStory {
  blob: BlobContent | number | 'pending';
  failingLevels?: string[];
  treeStatus?: number;
}

function blobRepositories({ blob, failingLevels = [], treeStatus }: BlobStory): Partial<RepositoriesService> {
  const fail = (status: number) => throwError(() => new HttpErrorResponse({ status, statusText: status === 404 ? 'Not Found' : 'Error' }));
  return {
    ...fakeRepositoriesService(POPULATED),
    blobAt: (): Observable<BlobContent> => (blob === 'pending' ? NEVER : typeof blob === 'number' ? fail(blob) : of(blob)),
    treeAt: (_id: string, _ref: string, path: string[]): Observable<TreeEntry[]> => {
      const key = path.join('/');
      if (failingLevels.includes(key)) return fail(500);
      if (LEVELS[key]) return of(LEVELS[key]);
      return treeStatus ? fail(treeStatus) : of(OTHER_FOLDER);
    },
  };
}

const text = (content: string): BlobContent => ({ sha: '9f3c2a1', size: new TextEncoder().encode(content).length, isBinary: false, content });

/** Real-layout check jsdom cannot make: above 768px the navigator is a column beside the file card, else above it. */
async function expectNavigatorPlacement({ canvasElement }: { canvasElement: HTMLElement }) {
  const layout = canvasElement.querySelector('gbt-page-layout') as HTMLElement;
  const nav = canvasElement.querySelector<HTMLElement>('.repository-blob-view__nav');
  const card = canvasElement.querySelector('.repository-blob-view__card') as HTMLElement;
  const toggle = canvasElement.querySelector('.repository-blob-view__nav-toggle') as HTMLElement;
  if (!nav) {
    await expect(card.classList, 'navigator missing outside the not-found page').toContain('repository-blob-view__not-found');
    await expect(Math.round(card.getBoundingClientRect().width)).toBe(Math.round(layout.getBoundingClientRect().width));
    return;
  }
  const navBox = nav.getBoundingClientRect();
  const cardBox = card.getBoundingClientRect();
  if (layout.getBoundingClientRect().width > 768) {
    await expect(Math.round(navBox.top), 'navigator level with the file card').toBe(Math.round(cardBox.top));
    await expect(navBox.right, 'navigator left of the file card').toBeLessThanOrEqual(cardBox.left);
    await expect(getComputedStyle(toggle).display, 'no disclosure button').toBe('none');
  } else {
    await expect(navBox.bottom, 'navigator above the file card').toBeLessThanOrEqual(cardBox.top);
    await expect(getComputedStyle(toggle).display, 'disclosure button').not.toBe('none');
  }
}

/** A file inside the app shell's content area. Above about 768px of layout width, the navigator is a column. */
const meta: Meta<RepositoryBlobView> = {
  title: 'Repositories/RepositoryBlobView',
  component: RepositoryBlobView,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withRouterAndIcons, inShellContentArea],
  args: { repositoryId: REPO.id, ref: 'main', blobPath: ['frontend', 'src', 'main.ts'] },
  play: expectNavigatorPlacement,
};

export default meta;
type Story = StoryObj<RepositoryBlobView>;

export const TypeScriptFile: Story = {
  decorators: [withRepository(blobRepositories({ blob: text(MAIN_TS) }))],
};

export const LongFileWithLongLines: Story = {
  args: { blobPath: ['crates', 'ferrisgit-api', 'src', 'routes', 'repositories.rs'] },
  decorators: [withRepository(blobRepositories({ blob: text(longRustFile()) }))],
};

export const MarkdownFile: Story = {
  args: { blobPath: ['README.md'] },
  decorators: [withRepository(blobRepositories({ blob: text(README) }))],
};

export const DeepPath: Story = {
  args: { blobPath: ['frontend', 'src', 'app', 'shared', 'code-view', 'code-view.ts'] },
  decorators: [withRepository(blobRepositories({ blob: text(CODE_VIEW_TS) }))],
};

export const NavigatorLevelFailed: Story = {
  args: { blobPath: ['frontend', 'src', 'app', 'shared', 'code-view', 'code-view.ts'] },
  decorators: [withRepository(blobRepositories({ blob: text(CODE_VIEW_TS), failingLevels: ['frontend/src/app'] }))],
};

export const Binary: Story = {
  args: { blobPath: ['frontend', 'src', 'assets', 'logo.png'] },
  decorators: [withRepository(blobRepositories({ blob: { sha: 'a1', size: 204_800, isBinary: true, content: null } }))],
};

export const TooLarge: Story = {
  args: { blobPath: ['scripts', 'dump.sql'] },
  decorators: [withRepository(blobRepositories({ blob: { sha: 'a1', size: 5_000_000, isBinary: false, content: null } }))],
};

export const NotFound: Story = {
  args: { ref: 'n-existe-pas', blobPath: ['docs', 'guide.md'] },
  decorators: [withRepository(blobRepositories({ blob: 404, treeStatus: 404 }))],
};

export const LoadFailed: Story = {
  decorators: [withRepository(blobRepositories({ blob: 500 }))],
};

export const Loading: Story = {
  decorators: [
    withRepository({
      ...blobRepositories({ blob: 'pending' }),
      getById: () => NEVER,
      treeAt: () => NEVER,
    }),
  ],
};

export const Mobile: Story = {
  args: { blobPath: ['crates', 'ferrisgit-api', 'src', 'routes', 'repositories.rs'] },
  decorators: [withRepository(blobRepositories({ blob: text(longRustFile()) })), atPhoneWidth],
};

export const MobileNavigatorOpen: Story = {
  ...Mobile,
  play: async (context) => {
    await expectNavigatorPlacement(context);
    const toggle = context.canvasElement.querySelector('.repository-blob-view__nav-toggle') as HTMLElement;
    await userEvent.click(toggle);
    await expect(toggle.getAttribute('aria-expanded')).toBe('true');
  },
};
