import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { CopyButton } from '@masmarino/gabarit/copy-button';
import { By } from '@angular/platform-browser';
import { PipelineEditor } from './pipeline-editor';
import { DefinitionDto, ParsedPipeline, RenderedPipeline, RepositoryPipelineFile } from './pipeline-definitions.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PIPELINE_TEMPLATES, tileById } from './pipeline-catalog';
import { PageTitleService } from '../../shell/page-title.service';
import { SettingsService } from '../../settings/settings.service';
import { PendingChanges } from '../../shared/pending-changes';
import { GbtToastService } from '@masmarino/gabarit/toaster';

const FILE_URL = '/api/repositories/repo-1/pipeline-definition';
const PROPOSAL_URL = '/api/repositories/repo-1/pipeline-definition/proposal';
const PARSE_URL = '/api/pipeline-definitions/parse';
const RENDER_URL = '/api/pipeline-definitions/render';
const PROFILE_URL = '/api/repositories/repo-1/pipeline-definition/profile';
const NOTHING_RECOGNISED = { projects: [], dockerfiles: [], helmCharts: [] };

const FILE = 'stages: [build, test]\njobs: ...';

const DEFINITION: DefinitionDto = {
  stages: ['build', 'test'],
  jobs: {
    compile: { stage: 'build', image: 'rust:1', script: ['cargo build'], variables: {}, needs: [], tags: [], cache: [] },
    unit: { stage: 'test', image: 'rust:1', script: ['cargo test'], variables: { RUST_LOG: 'debug' }, needs: ['compile'], tags: [], cache: [] },
  },
};

const fileBody = (yaml: string | null, overrides: Partial<RepositoryPipelineFile> = {}): RepositoryPipelineFile => ({ path: '.ferrisgit-ci.yml', branch: 'main', baseSha: 'tip1', yaml, ...overrides });
const parsed = (overrides: Partial<ParsedPipeline> = {}): ParsedPipeline => ({ definition: DEFINITION, problems: [], warnings: [], ignoredFields: [], hasComments: false, ...overrides });
const rendered = (overrides: Partial<RenderedPipeline> = {}): RenderedPipeline => ({ yaml: 'stages:\n- build\n- test\n', problems: [], warnings: [], ...overrides });

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

type Role = 'owner' | 'contributor' | 'maintainer' | 'reader';

/** The members the specs reach into: the component keeps them protected. */
interface Internals {
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
  modeOptions(): { value: string; disabled: boolean }[];
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
  onBeforeUnload(event: Event): void;
}

describe('PipelineEditor', () => {
  function setup(role: Role | null = 'contributor', engine: string | null = null) {
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
  function opened(result: ParsedPipeline = parsed(), role: Role | null = 'contributor') {
    const ctx = setup(role);
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
    ctx.http.expectOne(PARSE_URL).flush(result);
    ctx.fixture.detectChanges();
    return ctx;
  }

  /** Waits out the pause before a change is sent, and answers the render request. */
  async function answerRender(ctx: ReturnType<typeof setup>, result: RenderedPipeline = rendered()) {
    await sleep(300);
    const request = ctx.http.expectOne(RENDER_URL);
    request.flush(result);
    ctx.fixture.detectChanges();
    return request;
  }

  /** ngModel writes a field's value a tick after the field exists. */
  async function settle(ctx: ReturnType<typeof setup>) {
    await ctx.fixture.whenStable();
    ctx.fixture.detectChanges();
  }

  const lanes = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.pipeline-editor__lane:not(.pipeline-editor__lane--new)'));
  const stageNames = (el: HTMLElement) => lanes(el).map((lane) => lane.querySelector<HTMLInputElement>('.pipeline-editor__stage-name input')!.value);
  const cardNames = (lane: HTMLElement) => Array.from(lane.querySelectorAll('.pipeline-editor__card-name')).map(text);
  const buttonNamed = (root: ParentNode, name: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === name)!;
  const card = (el: HTMLElement, job: string) => el.querySelector<HTMLElement>(`[data-job="${job}"]`)!;
  /** Opens the actions menu of a card and reads its items. */
  function menuItems(ctx: ReturnType<typeof setup>, job: string): string[] {
    card(ctx.el, job).querySelector<HTMLButtonElement>('.pipeline-editor__actions .gbt-menu__trigger')!.click();
    ctx.fixture.detectChanges();
    return Array.from(card(ctx.el, job).querySelectorAll('[role="menuitem"]'), text);
  }

  afterEach(() => {
    // Reading the repository is a bonus asked for in the background: a spec about something else lets it find nothing.
    TestBed.inject(HttpTestingController)
      .match(PROFILE_URL)
      .forEach((request) => request.flush(NOTHING_RECOGNISED));
    TestBed.inject(HttpTestingController).verify();
  });

  it("opens the repository's file as stages with their jobs, in order", async () => {
    const ctx = opened();
    await answerRender(ctx);

    expect(stageNames(ctx.el)).toEqual(['build', 'test']);
    expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
    expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit']);
    expect(text(ctx.el.querySelector('.pipeline-editor__summary'))).toBe('2 jobs dans 2 étapes');
    expect(text(ctx.el.querySelector('.pipeline-editor__needs'))).toBe('Après compile');
  });

  it("shows the YAML the server wrote, and copies exactly that", async () => {
    const ctx = opened();
    await answerRender(ctx, rendered({ yaml: 'stages:\n- build\njobs: {}\n' }));

    expect(text(ctx.el.querySelector('.pipeline-editor__yaml .code-view__code'))).toBe('stages: - build jobs: {}');
    expect(ctx.fixture.debugElement.query(By.directive(CopyButton)).componentInstance.value()).toBe('stages:\n- build\njobs: {}\n');
  });

  it('sets the page title', () => {
    opened();

    expect(TestBed.inject(PageTitleService).title()).toBe('Éditeur de pipeline');
  });

  it('starts from an empty pipeline when the repository has no file', async () => {
    const ctx = setup();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(null));
    ctx.fixture.detectChanges();
    await answerRender(ctx);

    expect(stageNames(ctx.el)).toEqual(['build', 'test']);
    expect(ctx.el.querySelectorAll('.pipeline-editor__card')).toHaveLength(0);
    expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain("n'a pas encore de fichier .ferrisgit-ci.yml");
  });

  it('says so, and starts empty, when the file is not valid YAML', async () => {
    const ctx = opened(parsed({ definition: null, problems: [{ code: 'invalid_yaml', message: 'invalid YAML: did not find expected node' }] }));
    await answerRender(ctx);

    expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain("Le fichier du dépôt n'a pas pu être lu.");
    expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain('did not find expected node');
    expect(stageNames(ctx.el)).toEqual(['build', 'test']);
  });

  it('warns about what a rewrite would lose: comments and fields the server ignores', async () => {
    const ctx = opened(parsed({ hasComments: true, ignoredFields: ['include', 'jobs.compile.when'] }));
    await answerRender(ctx);

    const notes = text(ctx.el.querySelector('.pipeline-editor__notes'));
    expect(notes).toContain('contient des commentaires');
    expect(notes).toContain('include, jobs.compile.when.');
  });

  it("offers a retry when the file cannot be loaded", () => {
    const ctx = setup();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(null, { status: 500, statusText: 'Server Error' });
    ctx.fixture.detectChanges();

    expect(text(ctx.el)).toContain("Le fichier n'a pas pu être chargé");
    const retry = Array.from(ctx.el.querySelectorAll('button')).find((b) => text(b) === 'Réessayer')!;
    retry.click();
    ctx.http.expectOne(FILE_URL).flush(fileBody(null));
  });

  it('is for contributors: a reader gets an explanation, not the editor', () => {
    const ctx = setup('reader');
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(null));
    ctx.fixture.detectChanges();

    expect(text(ctx.el)).toContain('Réservé aux contributeurs');
    expect(ctx.el.querySelector('.pipeline-editor__board')).toBeNull();
  });

  describe('moving jobs', () => {
    it('drops a job on another stage at the place it was dropped', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const dropped = { item: { data: 'compile' }, previousContainer: { id: 'a' }, container: { id: 'b' }, previousIndex: 0, currentIndex: 0 };

      ctx.internals.drop(dropped, 'test');
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[0])).toEqual([]);
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['compile', 'unit']);
      await answerRender(ctx);
    });

    it('does nothing when a job is dropped where it was', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const same = { id: 'a' };

      ctx.internals.drop({ item: { data: 'compile' }, previousContainer: same, container: same, previousIndex: 0, currentIndex: 0 }, 'build');

      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
    });

    it('moves a job to the end of a stage from its menu, and announces it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'compile']);
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe("Job compile déplacé vers l'étape test");
      await answerRender(ctx);
    });

    it('offers every card its actions, moving it to each other stage among them', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(menuItems(ctx, 'compile')).toEqual(['Modifier', 'Dupliquer', 'Déplacer vers test', 'Supprimer']);
    });
  });

  describe('stages', () => {
    it('adds a stage from the form and clears the field', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const input = ctx.el.querySelector<HTMLInputElement>('.pipeline-editor__new-stage input')!;
      input.value = 'deploy';
      input.dispatchEvent(new Event('input'));
      ctx.fixture.detectChanges();

      ctx.el.querySelector<HTMLFormElement>('.pipeline-editor__new-stage')!.dispatchEvent(new Event('submit'));
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['build', 'test', 'deploy']);
      await answerRender(ctx);
    });

    it('renames a stage and its jobs follow', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.rename('build', 'compile-all');
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['compile-all', 'test']);
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
      await answerRender(ctx);
    });
  });

  describe('editing a job', () => {
    it('opens a card in the drawer', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.el.querySelector<HTMLButtonElement>('[data-job="unit"] .pipeline-editor__card-open')!.click();
      ctx.fixture.detectChanges();
      // ngModel writes its value a tick after the form is created.
      await ctx.fixture.whenStable();
      ctx.fixture.detectChanges();

      expect(ctx.internals.selected()).toBe('unit');
      const fields = Array.from(ctx.el.querySelectorAll<HTMLInputElement>('fg-pipeline-job-form input')).map((input) => input.value);
      expect(fields.slice(0, 2)).toEqual(['unit', 'rust:1']);
    });

    it('adds a job to a stage and opens it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      Array.from(lanes(ctx.el)[1].querySelectorAll('button')).find((b) => text(b) === 'Ajouter un job')!.click();
      ctx.fixture.detectChanges();
      ctx.internals.chooseTile(tileById('custom')!);
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'job']);
      expect(ctx.internals.selected()).toBe('job');
      await answerRender(ctx);
    });

    it('follows a job to its new name', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', {});
      ctx.el.querySelector<HTMLButtonElement>('[data-job="compile"] .pipeline-editor__card-open')!.click();

      ctx.internals.patchJob('compile', { name: 'build-all' });
      ctx.fixture.detectChanges();

      expect(ctx.internals.selected()).toBe('build-all');
      expect(ctx.internals.state().jobs.find((j) => j.name === 'unit')).toBeTruthy();
      await answerRender(ctx);
    });

    it('sends the change to the server after a pause, as the definition', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { image: 'node:22' });
      ctx.fixture.detectChanges();
      const request = await answerRender(ctx);

      expect(request.request.body.definition.jobs.compile.image).toBe('node:22');
      expect(request.request.body.definition.jobs.unit.needs).toEqual(['compile']);
    });

    it('sends one request for a burst of changes', async () => {
      const ctx = opened();
      await answerRender(ctx);

      for (const image of ['n', 'no', 'nod', 'node']) {
        ctx.internals.patchJob('compile', { image });
        ctx.fixture.detectChanges();
      }
      const request = await answerRender(ctx);

      expect(request.request.body.definition.jobs.compile.image).toBe('node');
    });

    it('removes a job and closes the drawer', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.el.querySelector<HTMLButtonElement>('[data-job="compile"] .pipeline-editor__card-open')!.click();
      ctx.fixture.detectChanges();

      Array.from(ctx.el.querySelectorAll('fg-pipeline-job-form button')).find((b) => text(b) === 'Supprimer le job')!.dispatchEvent(new Event('click'));
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[0])).toEqual([]);
      expect(ctx.internals.selected()).toBeNull();
      expect(ctx.internals.state().jobs.find((j) => j.name === 'unit')).toMatchObject({ stage: 'test' });
      await answerRender(ctx);
    });
  });

  describe('what the server says', () => {
    it('lists the problems, marks the job and opens it from the list', async () => {
      const ctx = opened();
      await answerRender(
        ctx,
        rendered({ problems: [{ code: 'unknown_dependency', message: 'x', job: 'unit', dependency: 'ghost' }] }),
      );

      expect(text(ctx.el.querySelector('.pipeline-editor__issues'))).toBe("Le job « unit » dépend de « ghost », qui n'existe pas.");
      expect(text(ctx.el.querySelector('[data-job="unit"] gbt-badge'))).toBe('1 problème');
      expect(ctx.el.querySelector('[data-job="unit"]')?.classList).toContain('pipeline-editor__card--problem');

      ctx.el.querySelector<HTMLButtonElement>('.pipeline-editor__issue-link')!.click();
      expect(ctx.internals.selected()).toBe('unit');
    });

    it('lists the warnings apart from the problems', async () => {
      const ctx = opened();
      await answerRender(ctx, rendered({ warnings: [{ code: 'empty_image', job: 'compile' }] }));

      expect(text(ctx.el)).toContain("Le job « compile » n'a pas d'image.");
      expect(text(ctx.el)).toContain('À vérifier');
      expect(text(ctx.el)).not.toContain('À corriger');
    });

    it('says when the server could not produce the file, and keeps the pipeline', async () => {
      const ctx = opened();
      await sleep(300);
      ctx.http.expectOne(RENDER_URL).flush(null, { status: 500, statusText: 'Server Error' });
      ctx.fixture.detectChanges();

      expect(text(ctx.el)).toContain("Le serveur n'a pas pu produire le fichier");
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
    });
  });

  it('goes back to the pipeline list', () => {
    const ctx = opened();
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

    Array.from(ctx.el.querySelectorAll('button')).find((b) => text(b) === 'Retour aux pipelines')!.click();

    expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'pipelines']);
  });

  it('starts again from the repository file, once asked', async () => {
    const ctx = opened();
    await answerRender(ctx);
    ctx.internals.patchJob('compile', { image: 'changed' });
    ctx.fixture.detectChanges();

    buttonNamed(ctx.el, 'Revenir au fichier du dépôt').click();
    ctx.fixture.detectChanges();
    expect(Array.from(document.querySelectorAll('gbt-confirm-danger-modal'), text).join(' ')).toContain('Les modifications faites ici seront perdues');
    ctx.http.expectNone(FILE_URL);

    buttonNamed(document.body, 'Revenir au fichier').click();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
    ctx.http.expectOne(PARSE_URL).flush(parsed());
    ctx.fixture.detectChanges();

    expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.image).toBe('rust:1');
    await answerRender(ctx);
  });

  describe('editing the YAML', () => {
    const TYPED = 'stages: [build]\n# keep me\njobs: {}\n';

    /** Waits out the pause before typed YAML is checked, and answers with `result`. */
    async function answerParse(ctx: ReturnType<typeof setup>, result: ParsedPipeline) {
      await sleep(300);
      // A card change made just before is still on its way to the server: the answer does not matter here.
      ctx.http.match(RENDER_URL).forEach((request) => request.flush(rendered()));
      ctx.http.expectOne(PARSE_URL).flush(result);
      ctx.fixture.detectChanges();
    }

    async function inYaml(result: ParsedPipeline = parsed()) {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.setMode('yaml');
      ctx.fixture.detectChanges();
      await answerParse(ctx, result);
      return ctx;
    }

    it("opens on the repository's own text, comments included, when the cards have not been touched", async () => {
      const ctx = await inYaml();

      expect(ctx.el.querySelector('.pipeline-editor__board')).toBeNull();
      await settle(ctx);
      expect(ctx.el.querySelector<HTMLTextAreaElement>('.pipeline-editor__yaml-input textarea')!.value).toBe(FILE);
    });

    it("opens on the server's writing of the cards once they were changed", async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });

      ctx.internals.setMode('yaml');
      ctx.http.expectOne(RENDER_URL).flush(rendered({ yaml: 'written by the server\n' }));
      ctx.fixture.detectChanges();
      await answerParse(ctx, parsed());
      await settle(ctx);

      expect(ctx.el.querySelector<HTMLTextAreaElement>('.pipeline-editor__yaml-input textarea')!.value).toBe('written by the server\n');
    });

    it('checks what is typed with the server and lists every problem it finds', async () => {
      const ctx = await inYaml();

      ctx.internals.typeYaml(TYPED);
      await answerParse(ctx, parsed({ problems: [{ code: 'unknown_dependency', message: 'x', job: 'a', dependency: 'b' }] }));

      expect(text(ctx.el.querySelector('.pipeline-editor__issues'))).toContain('Le job « a » dépend de « b », qui n\'existe pas.');
      expect(ctx.internals.canSave()).toBe(false);
    });

    it('can be saved once it is valid and differs from the repository', async () => {
      const ctx = await inYaml();
      expect(ctx.internals.canSave()).toBe(false);

      ctx.internals.typeYaml(TYPED);
      await answerParse(ctx, parsed());

      expect(ctx.internals.canSave()).toBe(true);
    });

    it('cannot go back to the cards while the YAML cannot be read, and says why', async () => {
      const ctx = await inYaml();

      ctx.internals.typeYaml('jobs: [');
      await answerParse(ctx, parsed({ definition: null, problems: [{ code: 'invalid_yaml', message: 'invalid YAML: did not find expected node' }] }));

      expect(ctx.internals.modeOptions().find((option) => option.value === 'cards')?.disabled).toBe(true);
      expect(text(ctx.el.querySelector('.pipeline-editor__issues'))).toContain("Le fichier n'est pas un YAML valide : did not find expected node");
      ctx.internals.setMode('cards');
      expect(ctx.el.querySelector('.pipeline-editor__board')).toBeNull();
      expect(ctx.internals.canSave()).toBe(false);
    });

    it('brings the typed pipeline back as cards, and says what that rewrite dropped', async () => {
      const ctx = await inYaml();
      const typed: DefinitionDto = { stages: ['ship'], jobs: { push: { stage: 'ship', image: 'alpine', script: ['echo hi'], variables: {}, needs: [], tags: [], cache: [] } } };

      ctx.internals.typeYaml(TYPED);
      await answerParse(ctx, parsed({ definition: typed, hasComments: true, ignoredFields: ['include'] }));
      ctx.internals.setMode('cards');
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['ship']);
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['push']);
      const notes = text(ctx.el.querySelector('.pipeline-editor__notes'));
      expect(notes).toContain('Passer aux cartes a réécrit le fichier');
      expect(notes).toContain('commentaires');
      expect(notes).toContain('include');
      await answerRender(ctx);
    });

    it('keeps the repository file as it is when the YAML is left without a change', async () => {
      const ctx = await inYaml();

      ctx.internals.setMode('cards');
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['build', 'test']);
      expect(ctx.internals.canSave()).toBe(false);
    });
  });

  describe('saving', () => {
    async function edited() {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });
      await answerRender(ctx, rendered({ yaml: 'stages:\n- build\n' }));
      return ctx;
    }

    const headerSave = (el: HTMLElement) => Array.from(el.querySelectorAll('gbt-page-header button')).find((b) => text(b) === 'Proposer la modification') as HTMLButtonElement;

    it('is offered only once something changed', async () => {
      const ctx = opened();
      await answerRender(ctx);
      expect(headerSave(ctx.el).disabled).toBe(true);

      ctx.internals.patchJob('compile', { image: 'changed' });
      await answerRender(ctx);

      expect(headerSave(ctx.el).disabled).toBe(false);
    });

    it('is not offered while the server reports problems', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });

      await answerRender(ctx, rendered({ problems: [{ code: 'unknown_stage', message: 'x', job: 'compile', stage: 'nope' }] }));

      expect(ctx.internals.canSave()).toBe(false);
    });

    it('sends the file with the base it was opened at, then goes to the merge request', async () => {
      const ctx = await edited();
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      ctx.internals.openSave();
      ctx.fixture.detectChanges();
      ctx.internals.saveTitle.set('Run the build');
      ctx.internals.save();
      const request = ctx.http.expectOne(PROPOSAL_URL);
      request.flush({ branch: 'pipeline-editor/abc', commitSha: 'c1', mergeRequestId: 'mr-9' });

      expect(request.request.body).toEqual({ yaml: 'stages:\n- build\n', baseSha: 'tip1', title: 'Run the build', description: '' });
      expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'merge-requests', 'mr-9']);
    });

    it('says the file changed under the editor when the server answers 409', async () => {
      const ctx = await edited();
      ctx.internals.openSave();
      ctx.internals.save();

      ctx.http.expectOne(PROPOSAL_URL).flush({ error: 'changed' }, { status: 409, statusText: 'Conflict' });
      ctx.fixture.detectChanges();

      expect(text(ctx.fixture.nativeElement.ownerDocument.querySelector('gbt-modal'))).toContain("Le fichier a changé sur main depuis l'ouverture de l'éditeur");
    });

    it("shows the server's reason when it refuses the change", async () => {
      const ctx = await edited();
      ctx.internals.openSave();
      ctx.internals.save();

      ctx.http.expectOne(PROPOSAL_URL).flush({ error: 'the file is the same' }, { status: 400, statusText: 'Bad Request' });
      ctx.fixture.detectChanges();

      expect(text(ctx.fixture.nativeElement.ownerDocument.querySelector('gbt-modal'))).toContain('Le serveur refuse cette modification : the file is the same');
    });

    it('cannot be saved from a repository with no commit, and says what to do instead', async () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null, { branch: null, baseSha: null }));
      ctx.fixture.detectChanges();
      ctx.internals.rename('build', 'compile');
      await answerRender(ctx);

      expect(ctx.internals.canSave()).toBe(false);
      expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain("aucun commit");
    });

    it('names the file where the repository keeps it', async () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(FILE, { path: 'ci/pipeline.yml' }));
      ctx.http.expectOne(PARSE_URL).flush(parsed());
      ctx.fixture.detectChanges();
      await answerRender(ctx);

      expect(text(ctx.el.querySelector('.pipeline-editor__yaml-head code'))).toBe('ci/pipeline.yml');
    });
  });

  describe('adding jobs from tiles', () => {
    it('opens the catalogue for the stage whose button was clicked', async () => {
      const ctx = opened();
      await answerRender(ctx);

      Array.from(lanes(ctx.el)[0].querySelectorAll('button')).find((b) => text(b) === 'Ajouter un job')!.click();
      ctx.fixture.detectChanges();

      expect(ctx.internals.pickerStage()).toBe('build');
      expect(text(ctx.el.ownerDocument.querySelector('fg-pipeline-tile-picker .tile-picker__intro'))).toContain('étape build');
    });

    it('adds the chosen tile at the end of the stage, opens it, and says so', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');

      ctx.internals.chooseTile(tileById('go-test'));
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'test']);
      expect(ctx.internals.selected()).toBe('test');
      expect(ctx.internals.pickerStage()).toBeNull();
      expect(ctx.internals.announcement()).toBe("Job test ajouté à l'étape test");
      await answerRender(ctx);
    });
  });

  describe('adding a tile with questions', () => {
    it('builds the job from the answers', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');

      ctx.internals.chooseTile(tileById('k8s-image'), { namespace: 'prod', deployment: 'web', mode: 'restart' });
      ctx.fixture.detectChanges();

      const job = ctx.internals.state().jobs.find((j) => j.name === 'update-k8s') as unknown as { stage: string; image: string; script: string[] };
      expect(job.stage).toBe('test');
      expect(job.image).toBe('alpine:3.20');
      expect(job.script).toContain('kubectl rollout restart deployment/web -n prod');
      await answerRender(ctx);
    });
  });

  describe('starting from a template', () => {
    async function emptyRepository() {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null));
      ctx.fixture.detectChanges();
      return ctx;
    }

    it('offers the templates while the pipeline has no job', async () => {
      const ctx = await emptyRepository();
      await answerRender(ctx);

      expect(ctx.el.querySelectorAll('.starters__card')).toHaveLength(PIPELINE_TEMPLATES.length);
    });

    it('lays the template out as stages and jobs, and stops offering templates', async () => {
      const ctx = await emptyRepository();
      await answerRender(ctx);

      ctx.internals.chooseTemplate(PIPELINE_TEMPLATES.find((t) => t.id === 'rust'));
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['check', 'test']);
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['format', 'clippy']);
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['test']);
      expect(ctx.el.querySelector('.starters__card')).toBeNull();
      await answerRender(ctx);
    });

    it('does not offer templates over a pipeline that has jobs', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(ctx.el.querySelector('fg-pipeline-starters')).toBeNull();
    });
  });

  describe('help', () => {
    it('explains the pipeline, each stage and the new-stage form in bubbles', async () => {
      const ctx = opened();
      await answerRender(ctx);

      const labels = Array.from(ctx.el.querySelectorAll('fg-help-tip button')).map((b) => b.getAttribute('aria-label'));

      expect(labels).toEqual(expect.arrayContaining(['Aide : Comment ça marche', 'Aide : Une étape', 'Aide : Ajouter une étape']));
      expect(labels.filter((label) => label === 'Aide : Une étape')).toHaveLength(2);
    });

    it('explains the YAML instead when the YAML is shown', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.setMode('yaml');
      ctx.fixture.detectChanges();

      expect(Array.from(ctx.el.querySelectorAll('fg-help-tip button')).map((b) => b.getAttribute('aria-label'))).toContain('Aide : Le YAML');
    });
  });

  describe('variables and secrets', () => {
    const SECRETS_URL = '/api/repositories/repo-1/ci-variables';

    /** A maintainer opens a pipeline whose jobs read DEPLOY_TOKEN and API_KEY; the repository has API_KEY and UNUSED. */
    async function asMaintainer() {
      const ctx = setup('maintainer');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
      const withSecrets: DefinitionDto = {
        stages: ['build', 'test'],
        jobs: {
          compile: { stage: 'build', image: 'rust:1', script: ['curl $API_KEY'], variables: {}, needs: [], tags: [], cache: [] },
          unit: { stage: 'test', image: 'rust:1', script: ['curl "$DEPLOY_TOKEN" $API_KEY'], variables: {}, needs: [], tags: [], cache: [] },
        },
      };
      ctx.http.expectOne(PARSE_URL).flush(parsed({ definition: withSecrets }));
      ctx.fixture.detectChanges();
      ctx.http.expectOne(SECRETS_URL).flush([{ id: '1', key: 'API_KEY', masked: true }, { id: '2', key: 'UNUSED', masked: true }]);
      ctx.fixture.detectChanges();
      await answerRender(ctx);
      return ctx;
    }

    it('loads the secrets of the repository for a maintainer', async () => {
      const ctx = await asMaintainer();

      expect(ctx.internals.secrets()).toEqual(['API_KEY', 'UNUSED']);
    });

    it('does not ask for them when the person is only a contributor', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(ctx.internals.secrets()).toBeNull();
    });

    it('knows which secrets the jobs read that are missing, and which jobs read the existing ones', async () => {
      const ctx = await asMaintainer();

      expect(ctx.internals.missingSecretUses()).toEqual([{ name: 'DEPLOY_TOKEN', jobs: ['unit'] }]);
      expect(ctx.internals.existingSecretUses()).toEqual([
        { name: 'API_KEY', jobs: ['compile', 'unit'] },
        { name: 'UNUSED', jobs: [] },
      ]);
      expect(text(ctx.el.querySelector('.pipeline-editor__secrets-badge'))).toBe('1 à créer');
    });

    it('opens the secrets panel from the toolbar', async () => {
      const ctx = await asMaintainer();

      Array.from(ctx.el.querySelectorAll('button')).find((b) => text(b) === 'Variables et secrets')!.click();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(SECRETS_URL).flush([]);

      expect(ctx.el.ownerDocument.querySelector('fg-pipeline-secrets')).not.toBeNull();
    });

    it('turns a variable that is a secret into a secret of the repository, and removes it from the job', async () => {
      const ctx = await asMaintainer();
      ctx.internals.patchJob('compile', { variables: [{ key: 'REGISTRY_TOKEN', value: 'abc' }, { key: 'MODE', value: 'fast' }] });

      ctx.internals.convertToSecret('compile', 0);
      const request = ctx.http.expectOne(SECRETS_URL);
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ key: 'REGISTRY_TOKEN', value: 'abc', masked: true });
      request.flush({ id: '3', key: 'REGISTRY_TOKEN', masked: true });
      ctx.http.expectOne((r) => r.url === SECRETS_URL && r.method === 'GET').flush([{ id: '3', key: 'REGISTRY_TOKEN', masked: true }]);

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')).toMatchObject({ variables: [{ key: 'MODE', value: 'fast' }] });
      expect(ctx.internals.secrets()).toEqual(['REGISTRY_TOKEN']);
      await sleep(300);
      ctx.http.match(RENDER_URL).forEach((r) => r.flush(rendered()));
    });

    it('keeps the variable in the job when the secret could not be saved', async () => {
      const ctx = await asMaintainer();
      ctx.internals.patchJob('compile', { variables: [{ key: 'REGISTRY_TOKEN', value: 'abc' }] });

      ctx.internals.convertToSecret('compile', 0);
      ctx.http.expectOne(SECRETS_URL).flush(null, { status: 500, statusText: 'Server Error' });

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.variables).toEqual([{ key: 'REGISTRY_TOKEN', value: 'abc' }]);
      await sleep(300);
      ctx.http.match(RENDER_URL).forEach((r) => r.flush(rendered()));
    });
  });

  describe('undo and redo', () => {
    it('undoes the last change, says what it undid, and redoes it', async () => {
      const ctx = opened();
      await answerRender(ctx);
      expect(ctx.internals.canUndo()).toBe(false);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();
      expect(ctx.internals.undoTip()).toMatch(/^Annuler : déplacement du job compile \((⌘Z|Ctrl\+Z)\)$/);

      expect(ctx.internals.undo()).toBe(true);
      ctx.fixture.detectChanges();
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe('Annulé : déplacement du job compile');
      expect(ctx.internals.canRedo()).toBe(true);

      expect(ctx.internals.redo()).toBe(true);
      ctx.fixture.detectChanges();
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'compile']);
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe('Rétabli : déplacement du job compile');
      await answerRender(ctx);
    });

    it('makes one step of what is typed in one field of a job', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { image: 'r' });
      ctx.internals.patchJob('compile', { image: 'ru' });
      ctx.internals.patchJob('compile', { image: 'rust:2' });
      ctx.internals.undo();

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.image).toBe('rust:1');
      expect(ctx.internals.canUndo()).toBe(false);
      await answerRender(ctx);
    });

    it('makes a step of each dependency ticked, however quick', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('unit', { needs: [] });
      ctx.internals.patchJob('unit', { needs: ['compile'] });
      ctx.internals.undo();

      expect(ctx.internals.state().jobs.find((j) => j.name === 'unit')).toMatchObject({ needs: [] });
      expect(ctx.internals.canUndo()).toBe(true);
      await answerRender(ctx);
    });

    it('closes the drawer of a job that undoing took away', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');
      ctx.internals.chooseTile(tileById('custom'));
      expect(ctx.internals.selected()).not.toBeNull();

      ctx.internals.undo();

      expect(ctx.internals.selected()).toBeNull();
      expect(ctx.internals.state().jobs.map((j) => j.name)).toEqual(['compile', 'unit']);
      await answerRender(ctx);
    });

    it('answers ⌘Z and Ctrl+Y on the board, and leaves them to a field being typed in', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');

      const field = ctx.el.querySelector<HTMLInputElement>('.pipeline-editor__stage-name input')!;
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true }));
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.stage).toBe('test');

      const undoKey = new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true, cancelable: true });
      document.body.dispatchEvent(undoKey);
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.stage).toBe('build');
      expect(undoKey.defaultPrevented).toBe(true);

      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'y', ctrlKey: true, bubbles: true }));
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.stage).toBe('test');
      await answerRender(ctx);
    });

    it('is not offered over the YAML, which the text field undoes itself', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      await answerRender(ctx);

      ctx.internals.setMode('yaml');
      ctx.http.expectOne(RENDER_URL).flush(rendered());
      await sleep(300);
      ctx.http.expectOne(PARSE_URL).flush(parsed());
      ctx.fixture.detectChanges();

      expect(ctx.internals.canUndo()).toBe(false);
      expect(ctx.el.querySelector('.pipeline-editor__history')).toBeNull();
    });

    it('starts afresh when the repository file is opened again', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();

      buttonNamed(ctx.el, 'Revenir au fichier du dépôt').click();
      ctx.fixture.detectChanges();
      buttonNamed(document.body, 'Revenir au fichier').click();
      ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
      ctx.http.expectOne(PARSE_URL).flush(parsed());

      expect(ctx.internals.canUndo()).toBe(false);
      await answerRender(ctx);
    });

    it('does not offer to go back to the repository file while nothing changed', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(buttonNamed(ctx.el, 'Revenir au fichier du dépôt').disabled).toBe(true);
    });
  });

  describe('card actions', () => {
    it('duplicates a job right after it, and opens the copy', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.duplicate('compile');
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile', 'compile-2']);
      expect(ctx.internals.selected()).toBe('compile-2');
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe('Job compile copié en compile-2');
      await answerRender(ctx);
    });

    it('deletes a job from its menu without asking, and says how to undo it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      menuItems(ctx, 'unit');
      Array.from(card(ctx.el, 'unit').querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).find((b) => text(b) === 'Supprimer')!.click();
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual([]);
      expect(TestBed.inject(GbtToastService).toasts().map((t) => t.message)).toEqual([expect.stringMatching(/^Job « unit » supprimé\. (⌘Z|Ctrl\+Z) pour l'annuler\.$/)]);

      ctx.internals.undo();
      ctx.fixture.detectChanges();
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit']);
      await answerRender(ctx);
    });

    it('shows what a job is for, its last command, and how many come before it', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('unit', { script: ['cd crates', '', 'cargo test --doc'] });
      ctx.fixture.detectChanges();

      expect(text(card(ctx.el, 'compile').querySelector('.pipeline-editor__card-command'))).toBe('cargo build');
      expect(text(card(ctx.el, 'unit').querySelector('.pipeline-editor__card-command-text'))).toBe('cargo test --doc');
      expect(text(card(ctx.el, 'unit').querySelector('.pipeline-editor__card-more'))).toBe('+1');
      expect(card(ctx.el, 'compile').querySelector('.pipeline-editor__card-more')).toBeNull();
      await answerRender(ctx);
    });

    it('marks the jobs a pointed job waits for, and those that wait for it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.pointedJob.set('unit');
      ctx.fixture.detectChanges();
      expect(card(ctx.el, 'compile').dataset['relation']).toBe('waited');
      expect(card(ctx.el, 'unit').classList).toContain('pipeline-editor__card--pointed');

      ctx.internals.pointedJob.set('compile');
      ctx.fixture.detectChanges();
      expect(card(ctx.el, 'unit').dataset['relation']).toBe('waiting');

      card(ctx.el, 'compile').dispatchEvent(new MouseEvent('mouseleave'));
      ctx.fixture.detectChanges();
      expect(card(ctx.el, 'unit').dataset['relation']).toBeUndefined();
    });
  });

  describe('leaving with changes', () => {
    it('lets one leave freely while nothing changed', async () => {
      opened();

      expect(TestBed.inject(PendingChanges).canLeave()).toBe(true);
    });

    it('asks first once something changed, and stays when told to', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      await answerRender(ctx);

      const answer = TestBed.inject(PendingChanges).canLeave() as Promise<boolean>;
      ctx.fixture.detectChanges();
      expect(Array.from(document.querySelectorAll('gbt-confirm-danger-modal'), text).join(' ')).toContain("Vos modifications n'ont pas été proposées");

      buttonNamed(document.body, "Rester dans l'éditeur").click();
      await expect(answer).resolves.toBe(false);
    });

    it('leaves once confirmed', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      await answerRender(ctx);

      const answer = TestBed.inject(PendingChanges).canLeave() as Promise<boolean>;
      ctx.fixture.detectChanges();
      buttonNamed(document.body, 'Quitter sans proposer').click();

      await expect(answer).resolves.toBe(true);
    });

    it('asks the browser to confirm closing the tab, only with changes', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const untouched = new Event('beforeunload', { cancelable: true });
      ctx.internals.onBeforeUnload(untouched);
      expect(untouched.defaultPrevented).toBe(false);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      const touched = new Event('beforeunload', { cancelable: true });
      ctx.internals.onBeforeUnload(touched);
      expect(touched.defaultPrevented).toBe(true);
      await answerRender(ctx);
    });

    it('goes to the merge request without asking once the change is proposed', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'rust:2' });
      await answerRender(ctx);
      vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      ctx.internals.openSave();
      ctx.internals.save();
      ctx.http.expectOne(PROPOSAL_URL).flush({ mergeRequestId: 'mr-1', branch: 'pipeline-editor/1' });

      expect(TestBed.inject(PendingChanges).canLeave()).toBe(true);
    });

    it('stops being asked once the editor is gone', () => {
      const ctx = opened();
      ctx.internals.moveTo({ name: 'compile' }, 'test');

      ctx.fixture.destroy();

      expect(TestBed.inject(PendingChanges).canLeave()).toBe(true);
      ctx.http.match(RENDER_URL);
    });
  });

  describe('a pipeline made for the repository', () => {
    /** A Rust workspace with an Angular app in `web`, as the server would read it. */
    const PROFILE = {
      projects: [
        { kind: 'rust', dir: '', evidence: ['Cargo.toml'], workspace: true, toolchain: '1.86', sqlxOffline: false },
        { kind: 'node', dir: 'web', evidence: ['web/package.json', 'web/package-lock.json'], packageManager: 'npm', nodeVersion: '22', scripts: { test: 'ng test', build: 'ng build' }, framework: 'angular', testRunner: 'vitest' },
      ],
      dockerfiles: [''],
      helmCharts: [],
    };

    async function emptyRepository(role: Role = 'contributor') {
      const ctx = setup(role);
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null));
      ctx.fixture.detectChanges();
      return ctx;
    }

    it('reads the repository once the pipeline is empty, and proposes what it found first', async () => {
      const ctx = await emptyRepository();
      expect(text(ctx.el.querySelector('fg-pipeline-starters [role="status"]'))).toBe('Lecture du dépôt');

      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      ctx.fixture.detectChanges();

      const proposal = ctx.el.querySelector('[data-prediction]')!;
      expect(text(proposal.querySelector('h2'))).toBe('Pour ce dépôt : Rust et Angular');
      expect(text(proposal)).toContain("D'après les fichiers de la branche main");
      expect(Array.from(proposal.querySelectorAll('.starters__evidence code'), text)).toEqual(['Cargo.toml', 'web/package.json', 'web/package-lock.json']);
      expect(Array.from(proposal.querySelectorAll('.starters__stage'), text)).toEqual(['check', 'test', 'build']);
      expect(text(proposal.querySelector('.starters__notes'))).toContain('Un Dockerfile à la racine');
      expect(text(ctx.el.querySelector('#starters-title'))).toBe("Ou partir d'un modèle");
      await answerRender(ctx);
    });

    it('lays the proposal out on the board, and undoing takes it back', async () => {
      const ctx = await emptyRepository();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      await answerRender(ctx);
      ctx.fixture.detectChanges();

      buttonNamed(ctx.el, 'Utiliser cette pipeline').click();
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['check', 'test', 'build']);
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['rust-test', 'web-test']);
      expect(ctx.internals.state().jobs.find((j) => j.name === 'web-test')?.image).toBe('node:22');
      expect(ctx.el.querySelector('fg-pipeline-starters')).toBeNull();
      await answerRender(ctx);

      ctx.internals.undo();
      expect(ctx.internals.state().jobs).toEqual([]);
      await answerRender(ctx);
    });

    it('keeps the templates alone when the repository cannot be read or holds nothing known', async () => {
      const ctx = await emptyRepository();

      ctx.http.expectOne(PROFILE_URL).flush(null, { status: 500, statusText: 'Server Error' });
      ctx.fixture.detectChanges();

      expect(ctx.el.querySelector('[data-prediction]')).toBeNull();
      expect(text(ctx.el.querySelector('#starters-title'))).toBe("Partir d'un modèle");
      expect(ctx.el.querySelectorAll('.starters__card')).toHaveLength(PIPELINE_TEMPLATES.length);
      await answerRender(ctx);
    });

    it('does not read the repository for a pipeline that has jobs until a job is added', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.http.expectNone(PROFILE_URL);

      ctx.internals.pickerStage.set('test');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      ctx.fixture.detectChanges();

      const offered = Array.from(document.querySelectorAll<HTMLElement>('[data-suggestion]'), (tile) => tile.dataset['suggestion']);
      // `unit` already runs cargo test at the root: Rust's tests are not offered again.
      expect(offered).toEqual(['rust-format', 'rust-clippy', 'web-test', 'web-build']);
      expect(text(document.querySelector('[data-suggestion="web-test"]'))).toContain('npm test -- --watch=false');
    });

    it('adds a suggested job to the stage it was asked for, waiting only for what is there before it', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      const clippy = { name: 'rust-clippy', stage: 'check', image: 'rust:1.86', script: ['cargo clippy'], variables: [], needs: ['compile', 'rust-format'], tags: [], cache: [] };

      ctx.internals.chooseSuggested(clippy);
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'rust-clippy']);
      expect(ctx.internals.state().jobs.find((j) => j.name === 'rust-clippy')).toMatchObject({ stage: 'test', needs: ['compile'] });
      expect(ctx.internals.selected()).toBe('rust-clippy');
      expect(ctx.internals.suggestions().map((s) => s.job.name)).not.toContain('rust-clippy');
      await answerRender(ctx);
    });

    it('does not read the repository for a reader', async () => {
      const ctx = await emptyRepository('reader');

      ctx.http.expectNone(PROFILE_URL);
    });
  });

  describe('the ties between jobs', () => {
    it('draws one per need, as on a pipeline page', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(ctx.internals.links()).toEqual([{ from: 'compile', to: 'unit', highlighted: false, invalid: false }]);
      expect(Array.from(ctx.el.querySelectorAll('fg-pipeline-links path'), (path) => path.getAttribute('data-link'))).toEqual(['compile->unit']);
    });

    it('brings out the ties of the pointed job, from either end', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.pointedJob.set('compile');
      ctx.fixture.detectChanges();

      expect(ctx.internals.links()[0].highlighted).toBe(true);
      expect(ctx.el.querySelector('fg-pipeline-links path')!.hasAttribute('data-highlighted')).toBe(true);
    });

    it('marks a need the server refuses: a job of the same stage or a later one', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();

      // The moved card takes the focus, so its tie may also stand out: only whether it is refused matters here.
      expect(ctx.internals.links()).toEqual([expect.objectContaining({ from: 'compile', to: 'unit', invalid: true })]);
      expect(ctx.el.querySelector('fg-pipeline-links path')!.hasAttribute('data-invalid')).toBe(true);
      await answerRender(ctx);
    });

    it('leaves out a need of a job that does not exist, and hides the ties while a card is carried', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('unit', { needs: ['compile', 'gone'] });
      ctx.fixture.detectChanges();
      expect(ctx.internals.links().map((link) => link.from)).toEqual(['compile']);

      ctx.internals.dragging.set(true);
      ctx.fixture.detectChanges();

      expect(ctx.el.querySelector('fg-pipeline-links')).toBeNull();
      await answerRender(ctx);
    });
  });
});
