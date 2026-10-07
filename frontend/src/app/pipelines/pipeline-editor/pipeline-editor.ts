import { Component, DestroyRef, ElementRef, Injector, OnInit, afterNextRender, computed, effect, inject, input, signal, untracked } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { HttpErrorResponse } from '@angular/common/http';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkDragDrop, DragDropModule } from '@angular/cdk/drag-drop';
import { CdkScrollable } from '@angular/cdk/scrolling';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { CopyButton } from '@masmarino/gabarit/copy-button';
import { Drawer } from '@masmarino/gabarit/drawer';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtInput } from '@masmarino/gabarit/input';
import { Menu, MenuItem } from '@masmarino/gabarit/menu';
import { Modal } from '@masmarino/gabarit/modal';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { SegmentedControl } from '@masmarino/gabarit/segmented-control';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Tooltip } from '@masmarino/gabarit/tooltip';
import { Subject, catchError, debounceTime, map, of, switchMap } from 'rxjs';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { injectRepositoryPermissions } from '../../repositories/repository-role';
import { RepositorySettingsService } from '../../repositories/repository-settings.service';
import { SettingsService } from '../../settings/settings.service';
import { PageTitleService } from '../../shell/page-title.service';
import { PendingChanges } from '../../shared/pending-changes';
import {
  BuilderJob,
  BuilderState,
  NEW_PIPELINE,
  addStage,
  duplicateJob,
  insertJob,
  uniqueName,
  fromDefinition,
  jobsOf,
  moveJob,
  moveStage,
  possibleNeeds,
  removeJob,
  removeStage,
  renameStage,
  toDefinition,
  updateJob,
} from './pipeline-builder-model';
import { ParsedPipeline, PipelineDefinitionsService, RenderedPipeline, RepositoryPipelineFile, RepositoryProfile } from './pipeline-definitions.service';
import { missingJobs, predictPipeline } from './pipeline-prediction';
import { HelpTip } from './help-tip';
import { HELP } from './pipeline-help';
import { JobTile, ParamValues, PipelineTemplate, addTile, stateFromTemplate } from './pipeline-catalog';
import { PipelineJobForm } from './pipeline-job-form';
import { PipelineSecrets, SecretUse } from './pipeline-secrets';
import { PipelineStarters } from './pipeline-starters';
import { PipelineTilePicker } from './pipeline-tile-picker';
import { missingSecrets, secretUsage, wantedSecretNames } from './pipeline-references';
import { ProblemView, describeProblem, describeWarning } from './pipeline-problems';
import { emptyHistory, record, redo, undo } from './pipeline-history';

/** What the editor shows when the repository does not say where its pipeline file is: the engine's own default. */
const DEFAULT_PIPELINE_FILE = '.ferrisgit-ci.yml';

type EditorMode = 'cards' | 'yaml';

/** Why a save did not go through, in the words the dialog needs. */
type SaveFailure = 'changed' | 'refused' | 'failed';

const stageId = (index: number) => `pipeline-stage-${index}`;

/** A key typed in a field undoes what was typed there, not the board. */
const isTyping = (target: EventTarget | null) => target instanceof HTMLElement && target.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]') !== null;

/** How the person's keyboard says it: the shortcuts are shown, and announced, as they would press them. */
const onApple = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);

interface CardView {
  job: BuilderJob;
  /** What the server says is wrong with this job (warnings are not counted). */
  problemCount: number;
  menuLabel: string;
  moveTargets: string[];
  /** What the job is for: its last command, the ones before only set it up (`cd web`, `npm ci`). */
  mainCommand: string | null;
  /** How many commands come before it. */
  setupCommands: number;
  /** Its tie to the job under the pointer or the focus: one it waits for, or one that waits for it. */
  relation: 'waited' | 'waiting' | null;
}

interface LaneView {
  stage: string;
  index: number;
  id: string;
  headingId: string;
  cards: CardView[];
  canMoveBefore: boolean;
  canMoveAfter: boolean;
  removable: boolean;
}

/**
 * Edits a pipeline two ways that stay in step: as cards dragged between stages, or as the YAML itself. Reading and
 * writing the YAML is the server's parser, so what is shown is what the engine would run. Saving never writes to the
 * default branch: it opens a merge request, so the change is reviewed like any other.
 */
@Component({
  selector: 'fg-pipeline-editor',
  standalone: true,
  imports: [FormsModule, DragDropModule, CdkScrollable, PageLayout, PageHeader, Alert, Badge, Button, ConfirmDangerModal, CopyButton, Drawer, EmptyState, GbtInput, Menu, MenuItem, Modal, SegmentedControl, Skeleton, Textarea, Tooltip, HelpTip, PipelineJobForm, PipelineSecrets, PipelineStarters, PipelineTilePicker],
  templateUrl: './pipeline-editor.html',
  styleUrl: './pipeline-editor.scss',
  host: { '(document:keydown)': 'onKeydown($event)', '(window:beforeunload)': 'onBeforeUnload($event)' },
})
export class PipelineEditor implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private pageTitle = inject(PageTitleService);
  private router = inject(Router);
  private definitions = inject(PipelineDefinitionsService);
  private repositorySettings = inject(RepositorySettingsService);
  private appSettings = inject(SettingsService);
  private toast = inject(GbtToastService);
  private repoContext = inject(RepositoryContextService);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private injector = inject(Injector);

  protected readonly help = HELP;
  protected status = signal<'loading' | 'ready' | 'failed'>('loading');
  /** Whether the pipeline started from the repository's file, or from nothing because it has none. */
  protected source = signal<'file' | 'new'>('new');
  protected state = signal<BuilderState>(NEW_PIPELINE);
  /** What opening the file showed: what a rewrite loses, or why it could not be read. */
  protected loadNotes = signal<{ hasComments: boolean; ignoredFields: string[]; unreadable: string | null }>({ hasComments: false, ignoredFields: [], unreadable: null });
  /** What the repository's default branch has, and where: the editor's starting point and what a save is checked against. */
  protected file = signal<RepositoryPipelineFile | null>(null);
  protected filePath = computed(() => this.file()?.path ?? DEFAULT_PIPELINE_FILE);
  protected mode = signal<EditorMode>('cards');
  /** The YAML as typed, comments and all: it is saved as it is, not rewritten. */
  protected yamlText = signal('');
  private yamlTouched = signal(false);
  /** What the server made of the typed YAML. `null` until it has answered. */
  private yamlCheck = signal<ParsedPipeline | null>(null);
  /** What switching from the YAML to the cards dropped, so that the loss is told once it has happened. */
  protected switchLoss = signal<{ hasComments: boolean; ignoredFields: string[] } | null>(null);
  private loadedDefinition = signal('');
  protected rendered = signal<RenderedPipeline | null>(null);
  protected renderFailed = signal(false);
  protected selected = signal<string | null>(null);
  protected newStageName = signal('');
  protected announcement = signal('');

  /** Touch drags start after a 250ms press, so a swipe scrolls the board instead of picking a card up. */
  protected readonly dragStartDelay = { touch: 250, mouse: 0 };

  private permissions = injectRepositoryPermissions();
  protected canWrite = this.permissions.canWrite;
  protected roleKnown = computed(() => this.repoContext.current()?.role != null);
  /** The repository's secrets belong to maintainers: they alone can list them or create one. */
  protected canManageSecrets = this.permissions.canMaintain;

  /** What the cards went through, to undo and redo. The YAML has its own: the text field's. */
  private history = signal(emptyHistory<BuilderState>());
  protected canUndo = computed(() => this.mode() === 'cards' && this.history().past.length > 0);
  protected canRedo = computed(() => this.mode() === 'cards' && this.history().future.length > 0);
  private readonly apple = onApple();
  protected readonly undoKeys = this.apple ? '⌘Z' : 'Ctrl+Z';
  protected readonly redoKeys = this.apple ? '⇧⌘Z' : 'Ctrl+Y';
  protected undoTip = computed(() => {
    const label = this.history().past.at(-1)?.label;
    return label ? `Annuler : ${label} (${this.undoKeys})` : 'Rien à annuler';
  });
  protected redoTip = computed(() => {
    const label = this.history().future.at(-1)?.label;
    return label ? `Rétablir : ${label} (${this.redoKeys})` : 'Rien à rétablir';
  });

  /** The job under the pointer or the focus, whose ties to the others the board shows. */
  protected pointedJob = signal<string | null>(null);

  protected resetOpen = signal(false);
  protected leaveOpen = signal(false);
  private leaveAnswer: ((leave: boolean) => void) | null = null;
  /** Set once the change went out as a merge request: going to it loses nothing. */
  private proposed = false;
  /** Their names, or `null` while unknown (not a maintainer, or not loaded). Values are never sent back by the server. */
  protected secrets = signal<string[] | null>(null);
  protected engine = computed(() => this.appSettings.publicSettings()?.executionEngine ?? null);
  protected pickerStage = signal<string | null>(null);
  protected secretsOpen = signal(false);
  protected secretSuggestion = signal<string | null>(null);

  /** The secrets the jobs read that the repository does not have, with the jobs that read them. */
  protected missingSecretUses = computed<SecretUse[]>(() => {
    const secrets = this.secrets();
    if (secrets === null) {
      return [];
    }
    const uses = new Map<string, string[]>();
    for (const job of this.state().jobs) {
      for (const name of missingSecrets(job, wantedSecretNames(job), secrets)) {
        uses.set(name, [...(uses.get(name) ?? []), job.name]);
      }
    }
    return [...uses].map(([name, jobs]) => ({ name, jobs }));
  });
  protected existingSecretUses = computed<SecretUse[]>(() => {
    const secrets = this.secrets() ?? [];
    return [...secretUsage(this.state().jobs, secrets)].map(([name, jobs]) => ({ name, jobs }));
  });
  protected showStarters = computed(() => this.state().jobs.length === 0 && this.mode() === 'cards');

  /** What the repository is made of, read once, when a proposal is first useful: an empty pipeline, or a job to add. */
  private profile = signal<RepositoryProfile | null>(null);
  protected profileState = signal<'idle' | 'loading' | 'done'>('idle');
  protected prediction = computed(() => predictPipeline(this.profile()));
  /** The jobs made for this repository that the pipeline lacks, offered first when a job is added. */
  protected suggestions = computed(() => missingJobs(this.prediction(), this.state()));

  /** The server has answered about what is on screen: until it does, nothing can be saved. */
  private checked = computed(() => (this.mode() === 'yaml' ? this.yamlCheck() !== null : this.rendered() !== null));
  protected problems = computed<ProblemView[]>(() => ((this.mode() === 'yaml' ? this.yamlCheck()?.problems : this.rendered()?.problems) ?? []).map(describeProblem));
  protected warnings = computed<ProblemView[]>(() => ((this.mode() === 'yaml' ? this.yamlCheck()?.warnings : this.rendered()?.warnings) ?? []).map(describeWarning));
  /** The file as it would be saved: the typed text in YAML mode, the server's writing of the cards otherwise. */
  protected yaml = computed(() => (this.mode() === 'yaml' ? this.yamlText() : (this.rendered()?.yaml ?? '')));
  /** The typed YAML cannot be read: the cards cannot follow it until it is fixed. */
  protected yamlUnreadable = computed(() => this.mode() === 'yaml' && this.yamlCheck()?.definition === null);
  protected modeOptions = computed(() => [
    { value: 'cards' as EditorMode, label: 'Cartes', disabled: this.yamlUnreadable() || (this.mode() === 'yaml' && this.yamlCheck() === null) },
    { value: 'yaml' as EditorMode, label: 'YAML' },
  ]);

  /** Whether anything differs from the repository's file, in either mode: saving a file as it already is would be a no-op. */
  protected edited = computed(() => this.yamlTouched() || JSON.stringify(toDefinition(this.state())) !== this.loadedDefinition());
  /** An empty repository has no branch to open a merge request against. */
  protected hasBranch = computed(() => this.file()?.baseSha != null);
  protected canSave = computed(() => this.canWrite() && this.hasBranch() && this.edited() && this.checked() && this.problems().length === 0 && !this.yamlUnreadable());

  protected saveOpen = signal(false);
  protected saving = signal(false);
  protected saveFailure = signal<{ kind: SaveFailure; detail: string } | null>(null);
  protected saveTitle = signal('');
  protected saveDescription = signal('');

  protected laneIds = computed(() => this.state().stages.map((_, index) => stageId(index)));

  protected lanes = computed<LaneView[]>(() => {
    const state = this.state();
    const pointed = state.jobs.find((job) => job.name === this.pointedJob()) ?? null;
    const relation = (job: BuilderJob): CardView['relation'] => {
      if (!pointed || pointed.name === job.name) {
        return null;
      }
      return pointed.needs.includes(job.name) ? 'waited' : job.needs.includes(pointed.name) ? 'waiting' : null;
    };
    const counts = new Map<string, number>();
    for (const problem of this.problems()) {
      if (problem.job) {
        counts.set(problem.job, (counts.get(problem.job) ?? 0) + 1);
      }
    }
    return state.stages.map((stage, index) => {
      const jobs = jobsOf(state, stage);
      return {
        stage,
        index,
        id: stageId(index),
        headingId: `${stageId(index)}-title`,
        cards: jobs.map((job) => {
          const commands = job.script.filter((line) => line.trim() !== '');
          return {
            job,
            problemCount: counts.get(job.name) ?? 0,
            menuLabel: `Actions du job ${job.name}`,
            moveTargets: state.stages.filter((other) => other !== stage),
            mainCommand: commands.at(-1)?.trim() ?? null,
            setupCommands: Math.max(0, commands.length - 1),
            relation: relation(job),
          };
        }),
        canMoveBefore: index > 0,
        canMoveAfter: index < state.stages.length - 1,
        removable: jobs.length === 0,
      };
    });
  });

  protected selectedJob = computed(() => {
    const name = this.selected();
    return name === null ? null : (this.state().jobs.find((job) => job.name === name) ?? null);
  });
  protected selectedNeeds = computed(() => {
    const job = this.selectedJob();
    return job ? possibleNeeds(this.state(), job.name) : [];
  });
  /** What the server says about the open job, problems first. */
  protected selectedNotes = computed(() => {
    const job = this.selectedJob();
    return job ? [...this.problems().filter((problem) => problem.job === job.name), ...this.warnings().filter((warning) => warning.job === job.name)] : [];
  });

  protected summary = computed(() => {
    const jobs = this.state().jobs.length;
    const stages = this.state().stages.length;
    return `${jobs} ${jobs === 1 ? 'job' : 'jobs'} dans ${stages} ${stages === 1 ? 'étape' : 'étapes'}`;
  });

  private renderRequests = new Subject<BuilderState>();
  private yamlRequests = new Subject<string>();

  constructor() {
    // The server writes and checks the YAML, so a change is sent after a short pause rather than on every keystroke.
    this.renderRequests
      .pipe(
        debounceTime(250),
        switchMap((state) =>
          this.definitions.render(toDefinition(state)).pipe(
            map((rendered) => ({ rendered })),
            catchError(() => of({ rendered: null })),
          ),
        ),
        takeUntilDestroyed(),
      )
      .subscribe(({ rendered }) => {
        this.renderFailed.set(rendered === null);
        if (rendered) {
          this.rendered.set(rendered);
        }
      });
    this.yamlRequests
      .pipe(
        debounceTime(250),
        switchMap((yaml) => this.definitions.parse(yaml).pipe(catchError(() => of(null)))),
        takeUntilDestroyed(),
      )
      .subscribe((checked) => {
        this.renderFailed.set(checked === null);
        if (checked) {
          this.yamlCheck.set(checked);
        }
      });
    effect(() => {
      const state = this.state();
      if (this.status() === 'ready') {
        this.renderRequests.next(state);
      }
    });
    effect(() => {
      if (this.canManageSecrets() && this.status() === 'ready') {
        untracked(() => this.loadSecrets());
      }
    });
    effect(() => {
      if (this.status() === 'ready' && this.canWrite() && (this.showStarters() || this.pickerStage() !== null) && untracked(this.profileState) === 'idle') {
        untracked(() => this.loadProfile());
      }
    });
    const unregister = inject(PendingChanges).register(() => this.confirmLeave());
    inject(DestroyRef).onDestroy(() => {
      unregister();
      this.answerLeave(false);
    });
  }

  /** Every change to the cards goes through here, so that it can be undone. `typingKey` groups the keystrokes of one field. */
  private change(next: BuilderState, label: string, typingKey: string | null = null): void {
    const before = this.state();
    if (next === before) {
      return;
    }
    this.history.update((history) => record(history, before, label, typingKey, Date.now()));
    this.state.set(next);
  }

  protected undo(): boolean {
    const step = undo(this.history(), this.state());
    if (!step) {
      return false;
    }
    this.history.set(step.history);
    this.showStep(step.state, `Annulé : ${step.label}`);
    return true;
  }

  protected redo(): boolean {
    const step = redo(this.history(), this.state());
    if (!step) {
      return false;
    }
    this.history.set(step.history);
    this.showStep(step.state, `Rétabli : ${step.label}`);
    return true;
  }

  /** A drawer about a job or a stage that the step took away closes with it. */
  private showStep(state: BuilderState, announcement: string): void {
    this.state.set(state);
    if (this.selected() !== null && !state.jobs.some((job) => job.name === this.selected())) {
      this.selected.set(null);
    }
    if (this.pickerStage() !== null && !state.stages.includes(this.pickerStage()!)) {
      this.pickerStage.set(null);
    }
    this.announcement.set(announcement);
  }

  protected onKeydown(event: KeyboardEvent): void {
    const key = event.key.toLowerCase();
    if (!(event.metaKey || event.ctrlKey) || event.altKey || (key !== 'z' && key !== 'y') || event.defaultPrevented) {
      return;
    }
    if (this.mode() !== 'cards' || this.status() !== 'ready' || isTyping(event.target) || this.saveOpen() || this.resetOpen() || this.leaveOpen()) {
      return;
    }
    const done = key === 'y' || event.shiftKey ? this.redo() : this.undo();
    if (done) {
      event.preventDefault();
    }
  }

  /** Work that would be lost by leaving: something changed, and it has not gone out as a merge request. */
  private holdsWork(): boolean {
    return this.status() === 'ready' && this.edited() && !this.proposed;
  }

  /** Closing the tab or reloading: only the browser's own question can stop it. */
  protected onBeforeUnload(event: BeforeUnloadEvent): void {
    if (this.holdsWork()) {
      event.preventDefault();
      // Safari and older browsers still read this rather than the call above.
      event.returnValue = '';
    }
  }

  /** Asked by the router before any other page: leaving with changes is a choice made in the dialog. */
  private confirmLeave(): boolean | Promise<boolean> {
    if (!this.holdsWork()) {
      return true;
    }
    this.answerLeave(false);
    this.leaveOpen.set(true);
    return new Promise<boolean>((resolve) => (this.leaveAnswer = resolve));
  }

  protected answerLeave(leave: boolean): void {
    this.leaveOpen.set(false);
    const answer = this.leaveAnswer;
    this.leaveAnswer = null;
    answer?.(leave);
  }

  ngOnInit(): void {
    this.pageTitle.set('Éditeur de pipeline');
    if (this.appSettings.publicSettings() === null) {
      this.appSettings.loadPublic();
    }
    this.load();
  }

  protected load(): void {
    this.status.set('loading');
    this.history.set(emptyHistory());
    this.mode.set('cards');
    this.yamlTouched.set(false);
    this.yamlCheck.set(null);
    this.switchLoss.set(null);
    this.rendered.set(null);
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

  protected openList(): void {
    this.router.navigate(['/repositories', ...this.path(), '-', 'pipelines']);
  }

  /** A job dropped on a stage. The CDK index is its place among that stage's cards once dropped. */
  protected drop(event: CdkDragDrop<CardView[]>, stage: string): void {
    const name = event.item.data as string;
    if (event.previousContainer === event.container && event.previousIndex === event.currentIndex) {
      return;
    }
    this.change(moveJob(this.state(), name, stage, event.currentIndex), `déplacement du job ${name}`);
  }

  /** The way to move a job without dragging: it goes to the end of the stage, is announced, and focus follows it. */
  protected moveTo(job: BuilderJob, stage: string): void {
    this.change(moveJob(this.state(), job.name, stage, Number.MAX_SAFE_INTEGER), `déplacement du job ${job.name}`);
    this.announcement.set(`Job ${job.name} déplacé vers l'étape ${stage}`);
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>(`[data-job="${CSS.escape(job.name)}"] .pipeline-editor__card-open`)?.focus(), { injector: this.injector });
  }

  protected addNewStage(): void {
    const name = this.newStageName().trim();
    const next = addStage(this.state(), name);
    if (next === this.state()) {
      return;
    }
    this.change(next, `ajout de l'étape ${name}`);
    this.newStageName.set('');
    this.announcement.set(`Étape ${name} ajoutée`);
  }

  protected rename(stage: string, name: string): void {
    this.change(renameStage(this.state(), stage, name), `renommage de l'étape ${stage}`);
  }

  protected shiftStage(lane: LaneView, by: -1 | 1): void {
    this.change(moveStage(this.state(), lane.index, lane.index + by), `déplacement de l'étape ${lane.stage}`);
    this.announcement.set(`Étape ${lane.stage} déplacée`);
  }

  protected deleteStage(stage: string): void {
    this.change(removeStage(this.state(), stage), `suppression de l'étape ${stage}`);
    this.announcement.set(`Étape ${stage} supprimée`);
  }

  /** Asks what the job should do: every kind of job is a tile, the empty one included. */
  protected addJobTo(stage: string): void {
    this.pickerStage.set(stage);
  }

  protected closePicker(): void {
    this.pickerStage.set(null);
  }

  protected chooseTile(tile: JobTile, values: ParamValues = {}): void {
    const stage = this.pickerStage();
    if (stage === null) {
      return;
    }
    const { state, job } = addTile(this.state(), tile, stage, values);
    this.change(state, `ajout du job ${job.name}`);
    this.pickerStage.set(null);
    this.announcement.set(`Job ${job.name} ajouté à l'étape ${stage}`);
    // Open it: the commands are a starting point, and the image or the variables are what a person adjusts first.
    this.selected.set(job.name);
  }

  /** A proposal is a bonus: if the repository cannot be read, the generic templates and tiles are still there. */
  private loadProfile(): void {
    this.profileState.set('loading');
    this.definitions.repositoryProfile(this.repositoryId()).subscribe({
      next: (profile) => {
        this.profile.set(profile);
        this.profileState.set('done');
      },
      error: () => this.profileState.set('done'),
    });
  }

  protected usePrediction(): void {
    const prediction = this.prediction();
    if (!prediction) {
      return;
    }
    this.change(prediction.state, 'pipeline proposée pour ce dépôt');
    this.announcement.set(`Pipeline proposée appliquée : ${prediction.state.jobs.length} jobs dans ${prediction.state.stages.length} étapes`);
  }

  /**
   * A job made for this repository, in the stage it was asked for. It keeps what it waits for only where those jobs
   * exist in an earlier stage: the pipeline it lands in is not the predicted one.
   */
  protected chooseSuggested(predicted: BuilderJob): void {
    const stage = this.pickerStage();
    if (stage === null) {
      return;
    }
    const state = this.state();
    const rank = state.stages.indexOf(stage);
    const name = uniqueName(state.jobs.map((job) => job.name), predicted.name);
    const needs = predicted.needs.filter((need) => {
      const other = state.jobs.find((job) => job.name === need);
      return other !== undefined && state.stages.indexOf(other.stage) < rank;
    });
    this.change(insertJob(state, { ...predicted, name, stage, needs, script: [...predicted.script], variables: predicted.variables.map((row) => ({ ...row })), tags: [...predicted.tags], cache: [...predicted.cache] }), `ajout du job ${name}`);
    this.pickerStage.set(null);
    this.announcement.set(`Job ${name} ajouté à l'étape ${stage}`);
    this.selected.set(name);
  }

  protected chooseTemplate(template: PipelineTemplate): void {
    this.change(stateFromTemplate(template), `modèle ${template.title}`);
    this.announcement.set(`Modèle ${template.title} appliqué`);
  }

  private loadSecrets(): void {
    this.repositorySettings.listCiVariables(this.repositoryId()).subscribe({
      next: (variables) => this.secrets.set(variables.map((variable) => variable.key)),
      error: () => this.secrets.set(null),
    });
  }

  protected refreshSecrets(): void {
    this.loadSecrets();
  }

  protected openSecrets(name: string | null = null): void {
    this.secretSuggestion.set(name);
    this.secretsOpen.set(true);
  }

  /** A variable that holds a secret goes to the repository, encrypted, and leaves the file everyone reads. */
  protected convertToSecret(jobName: string, index: number): void {
    const row = this.state().jobs.find((job) => job.name === jobName)?.variables[index];
    if (!row || row.key.trim() === '' || row.value === '') {
      return;
    }
    this.repositorySettings.setCiVariable(this.repositoryId(), row.key.trim(), row.value, true).subscribe({
      next: () => {
        this.patchJob(jobName, { variables: (this.state().jobs.find((job) => job.name === jobName)?.variables ?? []).filter((_, i) => i !== index) });
        this.loadSecrets();
        this.toast.show(`« ${row.key.trim()} » est maintenant un secret du dépôt.`);
      },
      error: () => this.toast.show("Impossible d'enregistrer le secret.", 'error'),
    });
  }

  protected openJob(name: string): void {
    this.selected.set(name);
  }

  protected closeJob(): void {
    this.selected.set(null);
  }

  protected patchJob(name: string, patch: Partial<BuilderJob>): void {
    const before = this.state();
    const next = updateJob(before, name, patch);
    const renaming = patch.name !== undefined && patch.name.trim() !== name;
    // Keystrokes in one field of one job are one step. A rename (applied when its field is left) and a ticked dependency
    // are choices, each a step of its own.
    const typed = !renaming && patch.needs === undefined;
    this.change(next, renaming ? `renommage du job ${name}` : `modification du job ${name}`, typed ? `job:${name}:${Object.keys(patch).sort().join(',')}` : null);
    const requested = patch.name?.trim();
    // The drawer follows the job to its new name, when the name was accepted.
    if (requested && requested !== name && next.jobs.some((job) => job.name === requested) && !before.jobs.some((job) => job.name === requested)) {
      this.selected.set(requested);
    }
  }

  /** No question asked: the step can be undone, and the notice says how. */
  protected deleteJob(name: string): void {
    this.change(removeJob(this.state(), name), `suppression du job ${name}`);
    this.selected.set(null);
    this.announcement.set(`Job ${name} supprimé`);
    this.toast.show(`Job « ${name} » supprimé. ${this.undoKeys} pour l'annuler.`);
  }

  /** The copy opens in the drawer: a copy is made to be changed (another version, another target). */
  protected duplicate(name: string): void {
    const result = duplicateJob(this.state(), name);
    if (!result) {
      return;
    }
    this.change(result.state, `copie du job ${name}`);
    this.announcement.set(`Job ${name} copié en ${result.copy}`);
    this.selected.set(result.copy);
  }

  protected setMode(mode: EditorMode): void {
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

  protected typeYaml(yaml: string): void {
    this.yamlText.set(yaml);
    this.yamlTouched.set(true);
    this.yamlRequests.next(yaml);
  }

  protected openSave(): void {
    this.saveTitle.set(this.source() === 'new' ? 'Ajouter une pipeline' : 'Modifier la pipeline');
    this.saveDescription.set('');
    this.saveFailure.set(null);
    this.saveOpen.set(true);
  }

  protected closeSave(): void {
    if (!this.saving()) {
      this.saveOpen.set(false);
    }
  }

  /** Sends the file on a new branch with a merge request, then goes to it. */
  protected save(): void {
    const baseSha = this.file()?.baseSha;
    if (!baseSha || this.saving()) {
      return;
    }
    this.saving.set(true);
    this.saveFailure.set(null);
    this.definitions.propose(this.repositoryId(), { yaml: this.yaml(), baseSha, title: this.saveTitle().trim(), description: this.saveDescription().trim() }).subscribe({
      next: (proposal) => {
        this.proposed = true;
        this.saving.set(false);
        this.saveOpen.set(false);
        this.router.navigate(['/repositories', ...this.path(), '-', 'merge-requests', proposal.mergeRequestId]);
      },
      error: (error: HttpErrorResponse) => {
        this.saving.set(false);
        const detail = typeof error.error?.error === 'string' ? error.error.error : '';
        this.saveFailure.set({ kind: error.status === 409 ? 'changed' : error.status === 400 ? 'refused' : 'failed', detail });
      },
    });
  }

  /** Throwing the changes away cannot be undone: it is asked first, and only offered when there is something to lose. */
  protected askReset(): void {
    if (this.edited()) {
      this.resetOpen.set(true);
    }
  }

  /** Starts again from the repository's file, dropping what was changed here. */
  protected reset(): void {
    this.resetOpen.set(false);
    this.selected.set(null);
    this.saveOpen.set(false);
    this.load();
  }
}
