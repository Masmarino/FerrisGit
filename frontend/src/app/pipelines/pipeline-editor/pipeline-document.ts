import { WritableSignal, computed, effect, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { Observable, Subject, catchError, debounceTime, map, of, switchMap } from 'rxjs';
import { BuilderState, NEW_PIPELINE, fromDefinition, toDefinition } from './pipeline-builder-model';
import { ParsedPipeline, PipelineDefinitionsService, RenderedPipeline, RepositoryPipelineFile } from './pipeline-definitions.service';
import { createUndoStack } from './pipeline-history';
import { ProblemView, describeProblem, describeWarning } from './pipeline-problems';

/**
 * Asks the server about the latest of `requests` once they pause, rather than on every keystroke. Each answer comes with
 * the request it answers; `null` when the server could not answer.
 */
function askAfterPause<T, R>(requests: Observable<T>, ask: (request: T) => Observable<R>): Observable<{ request: T; answer: R | null }> {
  return requests.pipe(
    debounceTime(250),
    switchMap((request) =>
      ask(request).pipe(
        map((answer): R | null => answer),
        catchError(() => of(null)),
        map((answer) => ({ request, answer })),
      ),
    ),
  );
}

/** What the editor shows when the repository does not say where its pipeline file is: the engine's own default. */
const DEFAULT_PIPELINE_FILE = '.ferrisgit-ci.yml';

export type EditorMode = 'cards' | 'yaml';

/**
 * The pipeline being edited, in two views that stay in step: the cards, and the YAML as typed. Reading and writing the
 * YAML is the server's parser, so what is shown is what the engine would run. It knows where the file came from, what
 * the server says about what is on screen, and whether that differs from the repository's file. The page around it
 * (drawers, tiles, secrets, the save dialog) is the editor's.
 *
 * Made in an injection context: it follows its own changes, and stops with the component that made it.
 */
export class PipelineDocument {
  readonly status = signal<'loading' | 'ready' | 'failed'>('loading');
  /** Whether the pipeline started from the repository's file, or from nothing because it has none. */
  readonly source = signal<'file' | 'new'>('new');
  readonly state = signal<BuilderState>(NEW_PIPELINE);
  /** What opening the file showed: what a rewrite loses, or why it could not be read. */
  readonly loadNotes = signal<{ hasComments: boolean; ignoredFields: string[]; unreadable: string | null }>({ hasComments: false, ignoredFields: [], unreadable: null });
  /** What the repository's default branch has, and where: the starting point and what a save is checked against. */
  readonly file = signal<RepositoryPipelineFile | null>(null);
  readonly filePath = computed(() => this.file()?.path ?? DEFAULT_PIPELINE_FILE);
  readonly mode = signal<EditorMode>('cards');
  /** The YAML as typed, comments and all: it is saved as it is, not rewritten. */
  readonly yamlText = signal('');
  private yamlTouched = signal(false);
  /** What the server made of the typed YAML. `null` until it has answered. */
  private yamlCheck = signal<ParsedPipeline | null>(null);
  /** What switching from the YAML to the cards dropped, so that the loss is told once it has happened. */
  readonly switchLoss = signal<{ hasComments: boolean; ignoredFields: string[] } | null>(null);
  private loadedDefinition = signal('');
  readonly rendered = signal<RenderedPipeline | null>(null);
  /** The cards `rendered` was written from: a later change, or a render that failed since, makes it out of date. */
  private renderedState = signal<BuilderState | null>(null);
  readonly renderFailed = signal(false);

  /** What the cards went through, to undo and redo. The YAML has its own: the text field's. */
  readonly edits = createUndoStack(this.state);

  /**
   * The server has answered about what is on screen. With the cards, the file shown and saved is the server's writing of
   * them: it has to be of these very cards, or a save would propose an older file (after a change still being sent, or
   * one the server could not write).
   */
  private checked = computed(() =>
    this.mode() === 'yaml' ? this.yamlCheck() !== null : this.rendered() !== null && this.renderedState() === this.state() && !this.renderFailed(),
  );
  readonly problems = computed<ProblemView[]>(() => ((this.mode() === 'yaml' ? this.yamlCheck()?.problems : this.rendered()?.problems) ?? []).map(describeProblem));
  readonly warnings = computed<ProblemView[]>(() => ((this.mode() === 'yaml' ? this.yamlCheck()?.warnings : this.rendered()?.warnings) ?? []).map(describeWarning));
  /** The file as it would be saved: the typed text in YAML mode, the server's writing of the cards otherwise. */
  readonly yaml = computed(() => (this.mode() === 'yaml' ? this.yamlText() : (this.rendered()?.yaml ?? '')));
  /** The typed YAML cannot be read: the cards cannot follow it until it is fixed. */
  readonly yamlUnreadable = computed(() => this.mode() === 'yaml' && this.yamlCheck()?.definition === null);
  readonly modeOptions = computed(() => [
    { value: 'cards' as EditorMode, label: 'Cartes', disabled: this.yamlUnreadable() || (this.mode() === 'yaml' && this.yamlCheck() === null) },
    { value: 'yaml' as EditorMode, label: 'YAML' },
  ]);

  /** Whether anything differs from the repository's file, in either mode: saving a file as it already is would be a no-op. */
  readonly edited = computed(() => this.yamlTouched() || JSON.stringify(toDefinition(this.state())) !== this.loadedDefinition());
  /** An empty repository has no branch to open a merge request against. */
  readonly hasBranch = computed(() => this.file()?.baseSha != null);
  /** What is on screen can go out as a merge request: it changed, the server read it, and found nothing wrong. */
  readonly proposable = computed(() => this.hasBranch() && this.edited() && this.checked() && this.problems().length === 0 && !this.yamlUnreadable());

  private renderRequests = new Subject<BuilderState>();
  private yamlRequests = new Subject<string>();

  constructor(
    private definitions: PipelineDefinitionsService,
    private repositoryId: () => string,
  ) {
    // The server writes and checks the YAML: the cards are sent to it, and what is typed, after a short pause.
    askAfterPause(this.renderRequests, (state) => this.definitions.render(toDefinition(state)))
      .pipe(takeUntilDestroyed())
      .subscribe(({ request, answer }) => {
        this.keepAnswer(answer, this.rendered);
        if (answer !== null) {
          this.renderedState.set(request);
        }
      });
    askAfterPause(this.yamlRequests, (yaml) => this.definitions.parse(yaml))
      .pipe(takeUntilDestroyed())
      .subscribe(({ answer }) => this.keepAnswer(answer, this.yamlCheck));
    effect(() => {
      const state = this.state();
      if (this.status() === 'ready') {
        this.renderRequests.next(state);
      }
    });
  }

  /** A failed answer is said, and the last good one stays shown. */
  private keepAnswer<T>(answer: T | null, into: WritableSignal<T | null>): void {
    this.renderFailed.set(answer === null);
    if (answer !== null) {
      into.set(answer);
    }
  }

  /** Every change to the cards goes through here, so that it can be undone. `typingKey` groups the keystrokes of one field. */
  change(next: BuilderState, label: string, typingKey: string | null = null): void {
    this.edits.change(next, label, typingKey);
  }

  /** Opens the repository's file, or an empty pipeline when it has none; whatever was changed is dropped. */
  load(): void {
    this.status.set('loading');
    this.edits.clear();
    this.mode.set('cards');
    this.yamlTouched.set(false);
    this.yamlCheck.set(null);
    this.switchLoss.set(null);
    this.rendered.set(null);
    this.renderedState.set(null);
    this.definitions.repositoryFile(this.repositoryId()).subscribe({
      next: (file) => {
        this.file.set(file);
        // No file yet, or a repository with no commit: start from an empty pipeline.
        if (file.yaml === null) {
          this.start();
        } else {
          this.read(file.yaml);
        }
      },
      error: () => this.status.set('failed'),
    });
  }

  private read(yaml: string): void {
    this.definitions.parse(yaml).subscribe({
      next: (parsed) => {
        if (parsed.definition === null) {
          const detail = describeProblem(parsed.problems[0] ?? { code: 'invalid_yaml', message: '' }).message;
          this.loadNotes.set({ hasComments: false, ignoredFields: [], unreadable: detail });
          this.start();
          return;
        }
        this.loadNotes.set({ hasComments: parsed.hasComments, ignoredFields: parsed.ignoredFields, unreadable: null });
        this.state.set(fromDefinition(parsed.definition));
        this.loadedDefinition.set(JSON.stringify(toDefinition(this.state())));
        this.source.set('file');
        this.status.set('ready');
      },
      error: () => this.status.set('failed'),
    });
  }

  private start(): void {
    this.state.set(NEW_PIPELINE);
    this.loadedDefinition.set(JSON.stringify(toDefinition(NEW_PIPELINE)));
    this.source.set('new');
    this.status.set('ready');
  }

  setMode(mode: EditorMode): void {
    if (mode === this.mode()) {
      return;
    }
    if (mode === 'yaml') {
      this.showYaml();
    } else {
      this.showCards();
    }
  }

  /** The repository's own text while nothing was changed (it keeps its comments), the server's writing of the cards after. */
  private showYaml(): void {
    const original = this.file()?.yaml ?? null;
    const open = (yaml: string) => {
      this.yamlText.set(yaml);
      this.yamlCheck.set(null);
      this.switchLoss.set(null);
      this.mode.set('yaml');
      this.yamlRequests.next(yaml);
    };
    if (original !== null && JSON.stringify(toDefinition(this.state())) === this.loadedDefinition()) {
      open(this.yamlTouched() ? this.yamlText() : original);
      return;
    }
    // The cards changed a moment ago: ask for their YAML now rather than trust one that may still be on its way.
    this.definitions.render(toDefinition(this.state())).subscribe({
      next: (rendered) => open(rendered.yaml),
      error: () => this.renderFailed.set(true),
    });
  }

  private showCards(): void {
    const checked = this.yamlCheck();
    if (checked?.definition == null) {
      return;
    }
    if (this.yamlTouched()) {
      this.change(fromDefinition(checked.definition), 'modification du YAML');
      this.switchLoss.set(checked.hasComments || checked.ignoredFields.length > 0 ? { hasComments: checked.hasComments, ignoredFields: checked.ignoredFields } : null);
    }
    this.mode.set('cards');
  }

  typeYaml(yaml: string): void {
    this.yamlText.set(yaml);
    this.yamlTouched.set(true);
    this.yamlRequests.next(yaml);
  }
}
