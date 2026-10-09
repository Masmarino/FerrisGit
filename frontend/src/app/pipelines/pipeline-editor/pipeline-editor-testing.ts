import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, TestRequest, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { PipelineEditor } from './pipeline-editor';
import { DefinitionDto, ParsedPipeline, RenderedPipeline, RepositoryPipelineFile } from './pipeline-definitions.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { SettingsService } from '../../settings/settings.service';

// Shared by the editor's specs: the files and answers the server gives, the editor set up and opened, and helpers to
// read the board.
export const FILE_URL = '/api/repositories/repo-1/pipeline-definition';
export const PROPOSAL_URL = '/api/repositories/repo-1/pipeline-definition/proposal';
export const PARSE_URL = '/api/pipeline-definitions/parse';
export const RENDER_URL = '/api/pipeline-definitions/render';
export const PROFILE_URL = '/api/repositories/repo-1/pipeline-definition/profile';
export const NOTHING_RECOGNISED = { projects: [], dockerfiles: [], helmCharts: [] };

export const FILE = 'stages: [build, test]\njobs: ...';

export const DEFINITION: DefinitionDto = {
  stages: ['build', 'test'],
  jobs: {
    compile: { stage: 'build', image: 'rust:1', script: ['cargo build'], variables: {}, needs: [], tags: [], cache: [] },
    unit: { stage: 'test', image: 'rust:1', script: ['cargo test'], variables: { RUST_LOG: 'debug' }, needs: ['compile'], tags: [], cache: [] },
  },
};

export const fileBody = (yaml: string | null, overrides: Partial<RepositoryPipelineFile> = {}): RepositoryPipelineFile => ({ path: '.ferrisgit-ci.yml', branch: 'main', baseSha: 'tip1', yaml, ...overrides });
export const parsed = (overrides: Partial<ParsedPipeline> = {}): ParsedPipeline => ({ definition: DEFINITION, problems: [], warnings: [], ignoredFields: [], hasComments: false, ...overrides });
export const rendered = (overrides: Partial<RenderedPipeline> = {}): RenderedPipeline => ({ yaml: 'stages:\n- build\n- test\n', problems: [], warnings: [], ...overrides });

export const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
export const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

export type Role = 'owner' | 'contributor' | 'maintainer' | 'reader';

/** The members the specs reach into (the component keeps them protected). */
export interface Internals {
  drop(event: unknown, stage: string): void;
  moveTo(job: { name: string }, stage: string): void;
  rename(stage: string, name: string): void;
  patchJob(name: string, patch: Record<string, unknown>): void;
  state(): { stages: string[]; jobs: { name: string; stage: string; image: string; needs: string[]; variables: { key: string; value: string }[] }[] };
  selected(): string | null;
  announcement(): string;
  setMode(mode: 'cards' | 'yaml'): void;
  typeYaml(yaml: string): void;
  openSave(): void;
  save(): void;
  saveTitle: { set(title: string): void };
  canSave(): boolean;
  chooseTile(tile: unknown, values?: Record<string, unknown>): void;
  chooseTemplate(template: unknown): void;
  pickerStage: { (): string | null; set(stage: string | null): void };
  convertToSecret(job: string, index: number): void;
  missingSecretUses(): { name: string; jobs: string[] }[];
  existingSecretUses(): { name: string; jobs: string[] }[];
  secrets(): string[] | null;
  doc: { modeOptions(): { value: string; disabled: boolean }[] };
  undo(): boolean;
  redo(): boolean;
  canUndo(): boolean;
  canRedo(): boolean;
  undoTip(): string;
  duplicate(name: string): void;
  deleteJob(name: string): void;
  pointedJob: { set(name: string | null): void };
  usePrediction(): void;
  chooseSuggested(job: unknown): void;
  suggestions(): { job: { name: string } }[];
  links(): { from: string; to: string; highlighted: boolean; invalid: boolean }[];
  dragging: { set(value: boolean): void };
}

export function setup(role: Role | null = 'contributor', engine: string | null = null) {
  TestBed.configureTestingModule({
    providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: SettingsService, useValue: { publicSettings: signal(engine ? { executionEngine: engine } : null), loadPublic: vi.fn() } }],
  });
  if (role) {
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role, ancestors: [], groupId: null });
  }
  const fixture = TestBed.createComponent(PipelineEditor);
  fixture.componentRef.setInput('repositoryId', 'repo-1');
  fixture.componentRef.setInput('path', ['acme', 'widget']);
  const http = TestBed.inject(HttpTestingController);
  const el = fixture.nativeElement as HTMLElement;
  const internals = fixture.componentInstance as unknown as Internals;
  return { fixture, http, el, internals };
}

/** The file exists and parses to `result`. */
export function opened(result: ParsedPipeline = parsed(), role: Role | null = 'contributor') {
  const ctx = setup(role);
  ctx.fixture.detectChanges();
  ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
  ctx.http.expectOne(PARSE_URL).flush(result);
  ctx.fixture.detectChanges();
  return ctx;
}

/** Waits out the pause before a change is sent, and answers the render request. */
/**
 * The request to `url`, once the editor has sent it. Changes are sent after a short pause, which a busy machine
 * stretches, so this waits for the request itself rather than for a fixed time.
 */
export async function nextRequest(ctx: ReturnType<typeof setup>, url: string, within = 3000): Promise<TestRequest> {
  const deadline = Date.now() + within;
  for (;;) {
    const found = ctx.http.match(url);
    if (found.length > 1) {
      throw new Error(`${found.length} requests to ${url}, expected one`);
    }
    if (found.length === 1) {
      return found[0];
    }
    if (Date.now() > deadline) {
      throw new Error(`no request to ${url} within ${within} ms`);
    }
    await sleep(20);
  }
}

/** Waits for the render request of the latest change, and answers it. */
export async function answerRender(ctx: ReturnType<typeof setup>, result: RenderedPipeline = rendered()) {
  const request = await nextRequest(ctx, RENDER_URL);
  request.flush(result);
  ctx.fixture.detectChanges();
  return request;
}

/** ngModel writes a field's value a tick after the field exists. */
export async function settle(ctx: ReturnType<typeof setup>) {
  await ctx.fixture.whenStable();
  ctx.fixture.detectChanges();
}

export const lanes = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.pipeline-editor__lane:not(.pipeline-editor__lane--new)'));
export const stageNames = (el: HTMLElement) => lanes(el).map((lane) => lane.querySelector<HTMLInputElement>('.pipeline-editor__stage-name input')!.value);
export const cardNames = (lane: HTMLElement) => Array.from(lane.querySelectorAll('.pipeline-editor__card-name')).map(text);
export const buttonNamed = (root: ParentNode, name: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === name)!;
export const card = (el: HTMLElement, job: string) => el.querySelector<HTMLElement>(`[data-job="${job}"]`)!;
/** Opens the actions menu of a card and reads its items. */
export function menuItems(ctx: ReturnType<typeof setup>, job: string): string[] {
  card(ctx.el, job).querySelector<HTMLButtonElement>('.pipeline-editor__actions .gbt-menu__trigger')!.click();
  ctx.fixture.detectChanges();
  return Array.from(card(ctx.el, job).querySelectorAll('[role="menuitem"]'), text);
}

/**
 * Every spec of the editor ends with this check: no request left unanswered. Reading the repository is a background
 * bonus, so a spec about something else answers it with a repository where nothing is recognised.
 */
export function verifyRequestsAfterEach(): void {
  afterEach(() => {
    TestBed.inject(HttpTestingController)
      .match(PROFILE_URL)
      .forEach((request) => request.flush(NOTHING_RECOGNISED));
    TestBed.inject(HttpTestingController).verify();
  });
}
