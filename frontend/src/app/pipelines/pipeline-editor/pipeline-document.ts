import { WritableSignal, computed, effect, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { Observable, Subject, catchError, debounceTime, map, of, switchMap } from 'rxjs';
import { BuilderState, NEW_PIPELINE, fromDefinition, toDefinition } from './pipeline-builder-model';
import { ParsedPipeline, PipelineDefinitionsService, RenderedPipeline, RepositoryPipelineFile } from './pipeline-definitions.service';
import { createUndoStack } from './pipeline-history';
import { ProblemView, describeProblem, describeWarning } from './pipeline-problems';
import { t } from '../../shared/i18n/translator';

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

/** The path shown when the repository settings name none: the engine's default. */
const DEFAULT_PIPELINE_FILE = '.ferrisgit-ci.yml';

export type EditorMode = 'cards' | 'yaml';

/**
 * The pipeline being edited, in two views that stay in step: the cards, and the YAML as typed. The server's parser
 * reads and writes the YAML, so what is shown is exactly what the engine would run. This class knows where the file
 * came from, what the server says about what is on screen, and whether it differs from the repository's file. The page
 * around it (drawers, tiles, secrets, the save dialog) belongs to the editor component. Create it in an injection
 * context: it reacts to its own changes, and stops with the component that created it.
 */
export class PipelineDocument {
  readonly status = signal<'loading' | 'ready' | 'failed'>('loading');
  /** Whether the pipeline started from the repository's file, or from nothing because it has none. */
  readonly source = signal<'file' | 'new'>('new');
  readonly state = signal<BuilderState>(NEW_PIPELINE);
  /** What opening the file revealed: what a rewrite would lose, or why the file could not be read. */
  readonly loadNotes = signal<{ hasComments: boolean; ignoredFields: string[]; unreadable: string | null }>({ hasComments: false, ignoredFields: [], unreadable: null });
  /** The file on the default branch, and where it lives: the starting point, and what a save is checked against. */
  readonly file = signal<RepositoryPipelineFile | null>(null);
  readonly filePath = computed(() => this.file()?.path ?? DEFAULT_PIPELINE_FILE);
  readonly mode = signal<EditorMode>('cards');
  /** The YAML as typed, comments included. It is saved exactly as typed, never rewritten. */
  readonly yamlText = signal('');
  private yamlTouched = signal(false);
  /** What the server made of the typed YAML. `null` until it has answered. */
  private yamlCheck = signal<ParsedPipeline | null>(null);
  /** What switching from the YAML to the cards dropped, so that the loss can be shown once it has happened. */
  readonly switchLoss = signal<{ hasComments: boolean; ignoredFields: string[] } | null>(null);
  private loadedDefinition = signal('');
  readonly rendered = signal<RenderedPipeline | null>(null);
  /** The cards that `rendered` was produced from. A later change, or a render that failed since, makes it stale. */
  private renderedState = signal<BuilderState | null>(null);
  readonly renderFailed = signal(false);

  /** The undo history of the cards. The YAML has its own: the text field's. */
  readonly edits = createUndoStack(this.state);

  /**
   * Whether the server has answered for what is on screen. With the cards, the file shown and saved is the server's
   * rendering of them, so it has to come from these exact cards. Otherwise a save could propose an older file (while a
   * change is still on its way, or after the server failed to render one).
   */
  private checked = computed(() =>
    this.mode() === 'yaml' ? this.yamlCheck() !== null : this.rendered() !== null && this.renderedState() === this.state() && !this.renderFailed(),
  );
  readonly problems = computed<ProblemView[]>(() => ((this.mode() === 'yaml' ? this.yamlCheck()?.problems : this.rendered()?.problems) ?? []).map(describeProblem));
  readonly warnings = computed<ProblemView[]>(() => ((this.mode() === 'yaml' ? this.yamlCheck()?.warnings : this.rendered()?.warnings) ?? []).map(describeWarning));
  /** The file as it would be saved: the typed text in YAML mode, the server's writing of the cards otherwise. */
  readonly yaml = computed(() => (this.mode() === 'yaml' ? this.yamlText() : (this.rendered()?.yaml ?? '')));
  /** The typed YAML cannot be parsed, so the cards cannot follow it until it is fixed. */
  readonly yamlUnreadable = computed(() => this.mode() === 'yaml' && this.yamlCheck()?.definition === null);
  readonly modeOptions = computed(() => [
    { value: 'cards' as EditorMode, label: t('pipelines.editor.cards'), disabled: this.yamlUnreadable() || (this.mode() === 'yaml' && this.yamlCheck() === null) },
    { value: 'yaml' as EditorMode, label: 'YAML' },
  ]);

  /**
   * Whether anything differs from the repository's file, in either view. Proposing an unchanged file would be
   * pointless.
   */
  readonly edited = computed(() => this.yamlTouched() || JSON.stringify(toDefinition(this.state())) !== this.loadedDefinition());
  /** An empty repository has no branch to open a merge request against. */
  readonly hasBranch = computed(() => this.file()?.baseSha != null);
  /** What is on screen can be proposed: it changed, the server read it, and it found nothing wrong. */
  readonly proposable = computed(() => this.hasBranch() && this.edited() && this.checked() && this.problems().length === 0 && !this.yamlUnreadable());

  private renderRequests = new Subject<BuilderState>();
  private yamlRequests = new Subject<string>();

  constructor(
    private definitions: PipelineDefinitionsService,
    private repositoryId: () => string,
  ) {
    // The server writes and checks the YAML: the cards are sent to it, and so is what is typed, after a short pause.
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

  /** A failed answer is flagged, and the last good one stays on screen. */
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

  /**
   * Shows the repository's own text while nothing has changed (its comments are kept), and the server's rendering of
   * the cards after a change.
   */
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
    // The cards changed a moment ago: ask for their YAML now, rather than trust an answer that may still be on its way.
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
      this.change(fromDefinition(checked.definition), t('pipelines.editor.yamlEdit'));
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
