import { Component, ElementRef, Injector, OnInit, afterNextRender, computed, effect, inject, input, signal, untracked } from '@angular/core';
import { HttpErrorResponse } from '@angular/common/http';
import { NgTemplateOutlet } from '@angular/common';
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
import { CodeView } from '../../shared/code-view/code-view';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { injectRepositoryPermissions } from '../../repositories/repository-role';
import { RepositorySettingsService } from '../../repositories/repository-settings.service';
import { SettingsService } from '../../settings/settings.service';
import { PageTitleService } from '../../shell/page-title.service';
import { confirmLeaving } from '../../shared/pending-changes';
import { BuilderJob, addStage, duplicateJob, insertJob, uniqueName, moveJob, moveStage, possibleNeeds, removeJob, removeStage, renameStage, updateJob } from './pipeline-builder-model';
import { PipelineDefinitionsService, RepositoryProfile } from './pipeline-definitions.service';
import { EditorMode, PipelineDocument } from './pipeline-document';
import { missingJobs, predictPipeline } from './pipeline-prediction';
import { HelpTip } from './help-tip';
import { help } from './pipeline-help';
import { JobTile, ParamValues, PipelineTemplate, addTile, stateFromTemplate } from './pipeline-catalog';
import { PipelineJobForm } from './pipeline-job-form';
import { PipelineSecrets, SecretUse } from './pipeline-secrets';
import { PipelineStarters } from './pipeline-starters';
import { PipelineLinks } from './pipeline-links';
import { CardView, LaneView, boardLanes, boardLinks, stageId } from './pipeline-board';
import { PipelineTilePicker } from './pipeline-tile-picker';
import { missingSecrets, secretUsage, wantedSecretNames } from './pipeline-references';
import { REDO_KEYS, UNDO_KEYS, undoShortcut } from './pipeline-history';
import { TranslocoPipe } from '@jsverse/transloco';
import { t, tn } from '../../shared/i18n/translator';

/** Why a save failed, as the dialog explains it. */
type SaveFailure = 'changed' | 'refused' | 'failed';

/**
 * Edits a pipeline in two views that stay in step: cards dragged between stages, or the YAML itself (see
 * PipelineDocument). This component is the page around them: the board's gestures, the job drawer, the tiles, the
 * secrets and the pipeline proposed for the repository. Saving never writes to the default branch: it opens a merge
 * request, so the change is reviewed like any other.
 */
@Component({
  selector: 'fg-pipeline-editor',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, NgTemplateOutlet, DragDropModule, CdkScrollable, PageLayout, PageHeader, Alert, Badge, Button, ConfirmDangerModal, CopyButton, Drawer, EmptyState, GbtInput, Menu, MenuItem, Modal, SegmentedControl, Skeleton, Textarea, Tooltip, CodeView, HelpTip, PipelineJobForm, PipelineLinks, PipelineSecrets, PipelineStarters, PipelineTilePicker],
  templateUrl: './pipeline-editor.html',
  styleUrl: './pipeline-editor.scss',
  host: { '(document:keydown)': 'onKeydown($event)' },
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

  protected readonly help = help();
  /** The pipeline being edited, and what the server says about it. */
  protected readonly doc = new PipelineDocument(this.definitions, () => this.repositoryId());
  /** The cards. Almost every action on the page changes them. */
  protected readonly state = this.doc.state;
  protected selected = signal<string | null>(null);
  protected newStageName = signal('');
  protected announcement = signal('');

  /** Touch drags start after a 250ms press, so a swipe scrolls the board instead of picking a card up. */
  protected readonly dragStartDelay = { touch: 250, mouse: 0 };

  private permissions = injectRepositoryPermissions();
  protected canWrite = this.permissions.canWrite;
  protected roleKnown = computed(() => this.repoContext.current()?.role != null);
  /** Only maintainers can list the repository's secrets or create one. */
  protected canManageSecrets = this.permissions.canMaintain;

  protected canUndo = computed(() => this.doc.mode() === 'cards' && this.doc.edits.nextUndo() !== null);
  protected canRedo = computed(() => this.doc.mode() === 'cards' && this.doc.edits.nextRedo() !== null);
  protected undoTip = computed(() => {
    const label = this.doc.edits.nextUndo();
    return label ? t('pipelines.editor.undoTip', { label, keys: UNDO_KEYS }) : t('pipelines.editor.nothingToUndo');
  });
  protected redoTip = computed(() => {
    const label = this.doc.edits.nextRedo();
    return label ? t('pipelines.editor.redoTip', { label, keys: REDO_KEYS }) : t('pipelines.editor.nothingToRedo');
  });

  /** The job under the pointer or with the focus. The board highlights its links to the other jobs. */
  protected pointedJob = signal<string | null>(null);
  /**
   * True while a card is being dragged: the other cards shift under it, so the links are hidden until it is dropped.
   */
  protected dragging = signal(false);

  /** Every `needs` of the board, drawn as on a pipeline's page, with the links of the pointed job highlighted. */
  protected links = computed(() => boardLinks(this.state(), this.pointedJob()));

  protected resetOpen = signal(false);
  /** Set once the change has gone out as a merge request: leaving then loses nothing. */
  private proposed = false;
  /** Leaving with changes that were not proposed asks for confirmation first. */
  protected leaving = confirmLeaving(() => this.holdsWork());
  /**
   * The names of the repository's secrets, or `null` while unknown (not a maintainer, or not loaded yet). The server
   * never sends their values.
   */
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
  protected showStarters = computed(() => this.state().jobs.length === 0 && this.doc.mode() === 'cards');

  /**
   * What the repository is made of. It is read once, the first time a proposal is useful: when the pipeline is empty,
   * or when a job is being added.
   */
  private profile = signal<RepositoryProfile | null>(null);
  protected profileState = signal<'idle' | 'loading' | 'done'>('idle');
  /**
   * Nothing is proposed with Kubernetes: every proposed job works on the repository's files, and a Pod does not get
   * them.
   */
  protected prediction = computed(() => (this.engine() === 'kubernetes' ? null : predictPipeline(this.profile())));
  /** The jobs made for this repository that the pipeline lacks, offered first when a job is added. */
  protected suggestions = computed(() => missingJobs(this.prediction(), this.state()));

  protected canSave = computed(() => this.canWrite() && this.doc.proposable());

  protected saveOpen = signal(false);
  protected saving = signal(false);
  protected saveFailure = signal<{ kind: SaveFailure; detail: string } | null>(null);
  protected saveTitle = signal('');
  protected saveDescription = signal('');

  protected laneIds = computed(() => this.state().stages.map((_, index) => stageId(index)));

  protected lanes = computed<LaneView[]>(() => boardLanes(this.state(), this.doc.problems().map((problem) => problem.job), this.pointedJob()));

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
    return job ? [...this.doc.problems().filter((problem) => problem.job === job.name), ...this.doc.warnings().filter((warning) => warning.job === job.name)] : [];
  });

  protected summary = computed(() => {
    const jobs = this.state().jobs.length;
    const stages = this.state().stages.length;
    return t('pipelines.editor.summary', { jobs: tn('pipelines.editor.jobs', jobs), stages: tn('pipelines.editor.stages', stages) });
  });

  constructor() {
    effect(() => {
      if (this.canManageSecrets() && this.doc.status() === 'ready') {
        untracked(() => this.loadSecrets());
      }
    });
    effect(() => {
      const proposable = this.engine() !== 'kubernetes';
      if (proposable && this.doc.status() === 'ready' && this.canWrite() && (this.showStarters() || this.pickerStage() !== null) && untracked(this.profileState) === 'idle') {
        untracked(() => this.loadProfile());
      }
    });
  }

  /** Every change to the cards goes through the document, so that it can be undone. */
  private change(...args: Parameters<PipelineDocument['change']>): void {
    this.doc.change(...args);
  }

  protected undo(): boolean {
    return this.afterStep(this.doc.edits.undo(), t('pipelines.editor.undone'));
  }

  protected redo(): boolean {
    return this.afterStep(this.doc.edits.redo(), t('pipelines.editor.redone'));
  }

  /** When the step removed the job or the stage a drawer was showing, the drawer closes. The step is then announced. */
  private afterStep(label: string | null, verb: string): boolean {
    if (label === null) {
      return false;
    }
    const state = this.state();
    if (this.selected() !== null && !state.jobs.some((job) => job.name === this.selected())) {
      this.selected.set(null);
    }
    if (this.pickerStage() !== null && !state.stages.includes(this.pickerStage()!)) {
      this.pickerStage.set(null);
    }
    this.announcement.set(`${verb} : ${label}`);
    return true;
  }

  protected onKeydown(event: KeyboardEvent): void {
    const step = undoShortcut(event);
    if (step === null || this.doc.mode() !== 'cards' || this.doc.status() !== 'ready' || this.saveOpen() || this.resetOpen() || this.leaving.asking()) {
      return;
    }
    if (step === 'undo' ? this.undo() : this.redo()) {
      event.preventDefault();
    }
  }

  /** Work that leaving would lose: something changed, and it has not gone out as a merge request yet. */
  private holdsWork(): boolean {
    return this.doc.status() === 'ready' && this.doc.edited() && !this.proposed;
  }

  ngOnInit(): void {
    this.pageTitle.set(t('pipelines.editor.title'));
    if (this.appSettings.publicSettings() === null) {
      this.appSettings.loadPublic();
    }
    this.load();
  }

  protected load(): void {
    this.doc.load();
  }

  protected openList(): void {
    this.router.navigate(['/repositories', ...this.path(), '-', 'pipelines']);
  }

  /** A job dropped on a stage. The CDK index is its position among that stage's cards after the drop. */
  protected drop(event: CdkDragDrop<CardView[]>, stage: string): void {
    const name = event.item.data as string;
    if (event.previousContainer === event.container && event.previousIndex === event.currentIndex) {
      return;
    }
    this.change(moveJob(this.state(), name, stage, event.currentIndex), t('pipelines.editor.edits.moveJob', { name }));
  }

  /**
   * Moves a job without dragging: it goes to the end of the stage, the move is announced, and the focus follows the
   * card.
   */
  protected moveTo(job: BuilderJob, stage: string): void {
    this.change(moveJob(this.state(), job.name, stage, Number.MAX_SAFE_INTEGER), t('pipelines.editor.edits.moveJob', { name: job.name }));
    this.announcement.set(t('pipelines.editor.announce.jobMoved', { name: job.name, stage }));
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>(`[data-job="${CSS.escape(job.name)}"] .pipeline-editor__card-open`)?.focus(), { injector: this.injector });
  }

  protected addNewStage(): void {
    const name = this.newStageName().trim();
    const next = addStage(this.state(), name);
    if (next === this.state()) {
      return;
    }
    this.change(next, t('pipelines.editor.edits.addStage', { name }));
    this.newStageName.set('');
    this.announcement.set(t('pipelines.editor.announce.stageAdded', { name }));
  }

  protected rename(stage: string, name: string): void {
    this.change(renameStage(this.state(), stage, name), t('pipelines.editor.edits.renameStage', { name: stage }));
  }

  protected shiftStage(lane: LaneView, by: -1 | 1): void {
    this.change(moveStage(this.state(), lane.index, lane.index + by), t('pipelines.editor.edits.moveStage', { name: lane.stage }));
    this.announcement.set(t('pipelines.editor.announce.stageMoved', { name: lane.stage }));
  }

  protected deleteStage(stage: string): void {
    this.change(removeStage(this.state(), stage), t('pipelines.editor.edits.removeStage', { name: stage }));
    this.announcement.set(t('pipelines.editor.announce.stageRemoved', { name: stage }));
  }

  /** Opens the tile picker for the stage: every kind of job is a tile, the empty one included. */
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
    this.change(state, t('pipelines.editor.edits.addJob', { name: job.name }));
    this.pickerStage.set(null);
    this.announcement.set(t('pipelines.editor.announce.jobAdded', { name: job.name, stage }));
    // Open it straight away: the commands are a starting point, and the image or the variables are what people adjust
    // first.
    this.selected.set(job.name);
  }

  /** The proposal is a bonus: if the repository cannot be read, the generic templates and tiles still work. */
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
    this.change(prediction.state, t('pipelines.editor.edits.prediction'));
    this.announcement.set(
      t('pipelines.editor.announce.predictionApplied', {
        summary: t('pipelines.editor.summary', { jobs: tn('pipelines.editor.jobs', prediction.state.jobs.length), stages: tn('pipelines.editor.stages', prediction.state.stages.length) }),
      }),
    );
  }

  /**
   * Adds a job proposed for this repository to the stage the picker was opened for. It only keeps the dependencies that
   * exist in that stage or an earlier one, since the pipeline it lands in is not the proposed one.
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
      // The server accepts a need in the same stage or an earlier one.
      return other !== undefined && state.stages.indexOf(other.stage) <= rank;
    });
    this.change(insertJob(state, { ...predicted, name, stage, needs, script: [...predicted.script], variables: predicted.variables.map((row) => ({ ...row })), tags: [...predicted.tags], cache: [...predicted.cache] }), t('pipelines.editor.edits.addJob', { name }));
    this.pickerStage.set(null);
    this.announcement.set(t('pipelines.editor.announce.jobAdded', { name, stage }));
    this.selected.set(name);
  }

  protected chooseTemplate(template: PipelineTemplate): void {
    this.change(stateFromTemplate(template), t('pipelines.editor.edits.template', { name: template.title }));
    this.announcement.set(t('pipelines.editor.announce.templateApplied', { name: template.title }));
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

  /**
   * Moves a variable that holds a secret into the repository's encrypted secrets, and out of the file everyone can
   * read.
   */
  protected convertToSecret(jobName: string, index: number): void {
    const row = this.state().jobs.find((job) => job.name === jobName)?.variables[index];
    if (!row || row.key.trim() === '' || row.value === '') {
      return;
    }
    const key = row.key.trim();
    this.repositorySettings.setCiVariable(this.repositoryId(), key, row.value, true).subscribe({
      next: () => {
        // Look the job up by name, not by position: its variables may have changed while the secret was being saved. It
        // is a step of its own, so that it is never undone together with the keystrokes before it.
        const job = this.state().jobs.find((candidate) => candidate.name === jobName);
        if (job) {
          this.change(updateJob(this.state(), jobName, { variables: job.variables.filter((variable) => variable.key.trim() !== key) }), t('pipelines.editor.edits.toSecret', { name: key }));
        }
        this.loadSecrets();
        this.toast.show(t('pipelines.editor.nowSecret', { name: key }));
      },
      error: () => this.toast.show(t('pipelines.editor.secretFailed'), 'error'),
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
    // Keystrokes in one field of one job make a single step. A rename (applied when the field loses the focus) and a
    // ticked dependency are deliberate choices, each a step of its own.
    const typed = !renaming && patch.needs === undefined;
    this.change(next, renaming ? t('pipelines.editor.edits.renameJob', { name }) : t('pipelines.editor.edits.editJob', { name }), typed ? `job:${name}:${Object.keys(patch).sort().join(',')}` : null);
    const requested = patch.name?.trim();
    // When the new name was accepted, the drawer follows the job to it.
    if (requested && requested !== name && next.jobs.some((job) => job.name === requested) && !before.jobs.some((job) => job.name === requested)) {
      this.selected.set(requested);
    }
  }

  /** No confirmation: the deletion can be undone, and the notification says how. */
  protected deleteJob(name: string): void {
    this.change(removeJob(this.state(), name), t('pipelines.editor.edits.removeJob', { name }));
    this.selected.set(null);
    this.announcement.set(t('pipelines.editor.announce.jobRemoved', { name }));
    this.toast.show(t('pipelines.editor.jobRemovedToast', { name, keys: UNDO_KEYS }));
  }

  /** The copy opens in the drawer: a copy is usually made to be changed (another version, another target). */
  protected duplicate(name: string): void {
    const result = duplicateJob(this.state(), name);
    if (!result) {
      return;
    }
    this.change(result.state, t('pipelines.editor.edits.copyJob', { name }));
    this.announcement.set(t('pipelines.editor.announce.jobCopied', { name, copy: result.copy }));
    this.selected.set(result.copy);
  }

  protected setMode(mode: EditorMode): void {
    this.doc.setMode(mode);
  }

  protected typeYaml(yaml: string): void {
    this.doc.typeYaml(yaml);
  }

  protected openSave(): void {
    this.saveTitle.set(this.doc.source() === 'new' ? t('pipelines.editor.addPipeline') : t('pipelines.editor.editPipeline'));
    this.saveDescription.set('');
    this.saveFailure.set(null);
    this.saveOpen.set(true);
  }

  protected closeSave(): void {
    if (!this.saving()) {
      this.saveOpen.set(false);
    }
  }

  /** Pushes the file to a new branch with a merge request, then opens that merge request. */
  protected save(): void {
    const baseSha = this.doc.file()?.baseSha;
    if (!baseSha || this.saving()) {
      return;
    }
    this.saving.set(true);
    this.saveFailure.set(null);
    this.definitions.propose(this.repositoryId(), { yaml: this.doc.yaml(), baseSha, title: this.saveTitle().trim(), description: this.saveDescription().trim() }).subscribe({
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

  /** Discarding the changes cannot be undone, so it asks first, and is only offered when there is something to lose. */
  protected askReset(): void {
    if (this.doc.edited()) {
      this.resetOpen.set(true);
    }
  }

  /** Starts over from the repository's file, dropping the changes made here. */
  protected reset(): void {
    this.resetOpen.set(false);
    this.selected.set(null);
    this.saveOpen.set(false);
    this.load();
  }
}
