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
import { HELP } from './pipeline-help';
import { JobTile, ParamValues, PipelineTemplate, addTile, stateFromTemplate } from './pipeline-catalog';
import { PipelineJobForm } from './pipeline-job-form';
import { PipelineSecrets, SecretUse } from './pipeline-secrets';
import { PipelineStarters } from './pipeline-starters';
import { PipelineLinks } from './pipeline-links';
import { CardView, LaneView, boardLanes, boardLinks, stageId } from './pipeline-board';
import { PipelineTilePicker } from './pipeline-tile-picker';
import { missingSecrets, secretUsage, wantedSecretNames } from './pipeline-references';
import { REDO_KEYS, UNDO_KEYS, undoShortcut } from './pipeline-history';

/** Why a save did not go through, in the words the dialog needs. */
type SaveFailure = 'changed' | 'refused' | 'failed';

/**
 * Edits a pipeline two ways that stay in step: as cards dragged between stages, or as the YAML itself (the document,
 * pipeline-document.ts). This is the page around it: the board's gestures, the job drawer, the tiles, the secrets and
 * the proposal made for the repository. Saving never writes to the default branch: it opens a merge request, so the
 * change is reviewed like any other.
 */
@Component({
  selector: 'fg-pipeline-editor',
  standalone: true,
  imports: [FormsModule, NgTemplateOutlet, DragDropModule, CdkScrollable, PageLayout, PageHeader, Alert, Badge, Button, ConfirmDangerModal, CopyButton, Drawer, EmptyState, GbtInput, Menu, MenuItem, Modal, SegmentedControl, Skeleton, Textarea, Tooltip, CodeView, HelpTip, PipelineJobForm, PipelineLinks, PipelineSecrets, PipelineStarters, PipelineTilePicker],
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

  protected readonly help = HELP;
  /** The pipeline itself, with what the server says about it. */
  protected readonly doc = new PipelineDocument(this.definitions, () => this.repositoryId());
  /** The cards: what nearly every gesture of the page changes. */
  protected readonly state = this.doc.state;
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

  protected canUndo = computed(() => this.doc.mode() === 'cards' && this.doc.edits.nextUndo() !== null);
  protected canRedo = computed(() => this.doc.mode() === 'cards' && this.doc.edits.nextRedo() !== null);
  protected undoTip = computed(() => {
    const label = this.doc.edits.nextUndo();
    return label ? `Annuler : ${label} (${UNDO_KEYS})` : 'Rien à annuler';
  });
  protected redoTip = computed(() => {
    const label = this.doc.edits.nextRedo();
    return label ? `Rétablir : ${label} (${REDO_KEYS})` : 'Rien à rétablir';
  });

  /** The job under the pointer or the focus, whose ties to the others the board shows. */
  protected pointedJob = signal<string | null>(null);
  /** A card is being carried: the cards move under it, so the ties are hidden until it lands. */
  protected dragging = signal(false);

  /** Every `needs` of the board, drawn as on a pipeline's page; those of the pointed job stand out. */
  protected links = computed(() => boardLinks(this.state(), this.pointedJob()));

  protected resetOpen = signal(false);
  /** Set once the change went out as a merge request: going to it loses nothing. */
  private proposed = false;
  /** Leaving with changes that were not proposed asks first. */
  protected leaving = confirmLeaving(() => this.holdsWork());
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
  protected showStarters = computed(() => this.state().jobs.length === 0 && this.doc.mode() === 'cards');

  /** What the repository is made of, read once, when a proposal is first useful: an empty pipeline, or a job to add. */
  private profile = signal<RepositoryProfile | null>(null);
  protected profileState = signal<'idle' | 'loading' | 'done'>('idle');
  /** Nothing is proposed with Kubernetes: every job it would make works on the repository, which a Pod does not get. */
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
    return `${jobs} ${jobs === 1 ? 'job' : 'jobs'} dans ${stages} ${stages === 1 ? 'étape' : 'étapes'}`;
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
    return this.afterStep(this.doc.edits.undo(), 'Annulé');
  }

  protected redo(): boolean {
    return this.afterStep(this.doc.edits.redo(), 'Rétabli');
  }

  /** A drawer about a job or a stage that the step took away closes with it, and the step is announced. */
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

  /** Work that would be lost by leaving: something changed, and it has not gone out as a merge request. */
  private holdsWork(): boolean {
    return this.doc.status() === 'ready' && this.doc.edited() && !this.proposed;
  }

  ngOnInit(): void {
    this.pageTitle.set('Éditeur de pipeline');
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
   * exist in that stage or an earlier one: the pipeline it lands in is not the predicted one.
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
      // The server takes a need in the same stage or an earlier one.
      return other !== undefined && state.stages.indexOf(other.stage) <= rank;
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
    const key = row.key.trim();
    this.repositorySettings.setCiVariable(this.repositoryId(), key, row.value, true).subscribe({
      next: () => {
        // By its name, not its place: the job's variables may have changed while the secret was being saved. A step of
        // its own, so that it is never undone along with the keystrokes that came before it.
        const job = this.state().jobs.find((candidate) => candidate.name === jobName);
        if (job) {
          this.change(updateJob(this.state(), jobName, { variables: job.variables.filter((variable) => variable.key.trim() !== key) }), `passage de ${key} en secret`);
        }
        this.loadSecrets();
        this.toast.show(`« ${key} » est maintenant un secret du dépôt.`);
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
    this.toast.show(`Job « ${name} » supprimé. ${UNDO_KEYS} pour l'annuler.`);
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
    this.doc.setMode(mode);
  }

  protected typeYaml(yaml: string): void {
    this.doc.typeYaml(yaml);
  }

  protected openSave(): void {
    this.saveTitle.set(this.doc.source() === 'new' ? 'Ajouter une pipeline' : 'Modifier la pipeline');
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

  /** Throwing the changes away cannot be undone: it is asked first, and only offered when there is something to lose. */
  protected askReset(): void {
    if (this.doc.edited()) {
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
