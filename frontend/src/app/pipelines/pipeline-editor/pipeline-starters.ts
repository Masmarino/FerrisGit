import { Component, computed, input, output } from '@angular/core';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Icon } from '@masmarino/gabarit/icon';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { HelpTip } from './help-tip';
import { HELP } from './pipeline-help';
import { PIPELINE_TEMPLATES, PipelineTemplate } from './pipeline-catalog';
import { Prediction } from './pipeline-prediction';

/**
 * Whole pipelines to start an empty one from: a click lays out the stages and the jobs. The pipeline proposed for this
 * repository comes first, with the files it was read from. The generic templates stay below for whatever it did not
 * recognise. With Kubernetes, neither is offered: they all work on the repository's files, and a Pod gets no copy of
 * them.
 */
@Component({
  selector: 'fg-pipeline-starters',
  standalone: true,
  imports: [Alert, Button, Icon, HelpTip, Skeleton],
  templateUrl: './pipeline-starters.html',
  styleUrl: './pipeline-starters.scss',
})
export class PipelineStarters {
  /** The pipeline proposed for this repository, or `null` when nothing in it was recognised. */
  prediction = input<Prediction | null>(null);
  /** True while the repository is being read and its proposal is on its way. */
  predicting = input(false);
  /** The default branch the proposal was read from. */
  branch = input<string | null>(null);
  engine = input<string | null>(null);
  chosen = output<PipelineTemplate>();
  usePrediction = output<void>();

  /** The proposal's stages in order, each with its jobs. */
  protected readonly plan = computed(() => {
    const proposal = this.prediction();
    return proposal ? proposal.state.stages.map((name) => ({ name, jobs: proposal.state.jobs.filter((job) => job.stage === name).map((job) => job.name) })) : [];
  });
  protected readonly help = HELP;
  protected readonly templates = PIPELINE_TEMPLATES;
}
