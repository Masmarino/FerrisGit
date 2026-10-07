import { Component, computed, input, output } from '@angular/core';
import { Button } from '@masmarino/gabarit/button';
import { Icon } from '@masmarino/gabarit/icon';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { HelpTip } from './help-tip';
import { HELP } from './pipeline-help';
import { PIPELINE_TEMPLATES, PipelineTemplate } from './pipeline-catalog';
import { Prediction } from './pipeline-prediction';

/**
 * Whole pipelines to start from, for an empty one: a click lays out the stages and the jobs. The one made for this
 * repository comes first, with what it was read from; the generic templates stay below for what it did not recognise.
 */
@Component({
  selector: 'fg-pipeline-starters',
  standalone: true,
  imports: [Button, Icon, HelpTip, Skeleton],
  template: `
    <section class="starters" aria-labelledby="starters-title">
      @if (predicting()) {
        <div class="starters__prediction" aria-hidden="true">
          <gbt-skeleton width="16rem" height="1.25rem" />
          <gbt-skeleton width="100%" height="0.875rem" />
          <gbt-skeleton width="80%" height="0.875rem" />
        </div>
        <span class="sr-only" role="status">Lecture du dépôt</span>
      } @else if (prediction(); as proposal) {
        <div class="starters__prediction" data-prediction aria-labelledby="prediction-title" role="group">
          <h2 id="prediction-title" class="starters__title">Pour ce dépôt : {{ proposal.title }}</h2>
          <p class="starters__summary">
            D'après les fichiers de la branche {{ branch() ?? 'par défaut' }}. Chaque job reprend les versions, les outils et les scripts du projet ; vous pourrez tout changer ensuite.
          </p>
          <ul class="starters__reasons">
            @for (reason of proposal.reasons; track reason.text) {
              <li>
                <span class="starters__evidence">
                  @for (file of reason.evidence; track file) {
                    <code>{{ file }}</code>
                  }
                </span>
                <span>{{ reason.text }}</span>
              </li>
            }
          </ul>
          <ol class="starters__plan" aria-label="Étapes proposées, dans l'ordre">
            @for (stage of plan(); track stage.name) {
              <li>
                <span class="starters__stage">{{ stage.name }}</span>
                <span class="starters__jobs">
                  @for (job of stage.jobs; track job) {
                    <code>{{ job }}</code>
                  }
                </span>
              </li>
            }
          </ol>
          @if (proposal.notes.length > 0) {
            <ul class="starters__notes">
              @for (note of proposal.notes; track note) {
                <li>{{ note }}</li>
              }
            </ul>
          }
          <gbt-button class="starters__use" variant="primary" text="Utiliser cette pipeline" (clicked)="usePrediction.emit()" />
        </div>
      }
      <div class="starters__head">
        <h2 id="starters-title" class="starters__title">
          @if (prediction()) {
            Ou partir d'un modèle
          } @else {
            Partir d'un modèle
          }
        </h2>
        <fg-help-tip [help]="help.pipeline" />
      </div>
      <p class="starters__summary">Une pipeline complète pour votre type de projet. Vous pourrez tout changer ensuite, ou ajouter vos propres jobs.</p>
      <ul class="starters__list">
        @for (template of templates; track template.id) {
          <li>
            <button type="button" class="starters__card" [attr.data-template]="template.id" (click)="chosen.emit(template)">
              <gbt-icon [name]="template.icon" aria-hidden="true" />
              <span class="starters__name">{{ template.title }}</span>
              <span class="starters__what">{{ template.summary }}</span>
              <span class="starters__stages">{{ template.stages.join(' → ') }}</span>
            </button>
          </li>
        }
      </ul>
    </section>
  `,
  styles: `
    :host {
      display: block;
      margin-bottom: 1rem;
    }
    .starters {
      container: starters / inline-size;
      padding: 1rem;
      border: 1px dashed var(--gbt-hairline);
      border-radius: var(--site-border-radius);
    }
    /* What was made for this repository, on the family's rust rail: the generic templates below have none. */
    .starters__prediction {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      gap: 0.75rem;
      margin-bottom: 1.25rem;
      padding: 0 0 1.25rem 1rem;
      border-bottom: 1px solid var(--gbt-hairline);
      box-shadow: inset 2px 0 0 var(--accent);
    }
    .starters__prediction .starters__summary {
      margin: 0;
    }
    .starters__reasons,
    .starters__notes {
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
      margin: 0;
      padding: 0;
      list-style: none;
      font-size: 0.875rem;
    }
    .starters__reasons li {
      display: grid;
      grid-template-columns: minmax(0, 14rem) minmax(0, 1fr);
      gap: 0.25rem 1rem;
    }
    .starters__evidence,
    .starters__jobs {
      display: flex;
      flex-wrap: wrap;
      gap: 0.25rem 0.5rem;
      min-width: 0;
    }
    .starters__evidence code,
    .starters__jobs code {
      font-family: var(--gbt-font-mono, monospace);
      font-size: 0.8125rem;
      overflow-wrap: anywhere;
    }
    .starters__plan {
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem;
      margin: 0;
      padding: 0;
      list-style: none;
    }
    .starters__plan li {
      display: flex;
      flex-direction: column;
      gap: 0.25rem;
      min-width: 9rem;
      padding: 0.5rem 0.75rem;
      border: 1px solid var(--border-color);
      border-radius: var(--gbt-radius-tile, 4px);
      background: var(--bg-principal);
    }
    .starters__stage {
      font-weight: 600;
    }
    .starters__notes {
      color: var(--text-secondary);
      font-size: 0.8125rem;
    }
    /* The width the proposal is given, not the screen's: the editor can sit in a narrow column of a wide window. */
    @container starters (max-width: 40rem) {
      .starters__reasons li {
        grid-template-columns: minmax(0, 1fr);
      }
    }
    .starters__head {
      display: flex;
      align-items: center;
      gap: 0.25rem;
    }
    .starters__title {
      margin: 0;
      font-size: 1rem;
    }
    .starters__summary {
      margin: 0.25rem 0 0.75rem;
      color: var(--text-secondary);
      font-size: 0.875rem;
    }
    .starters__list {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
      gap: 0.75rem;
      margin: 0;
      padding: 0;
      list-style: none;
    }
    .starters__card {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      gap: 0.25rem;
      width: 100%;
      height: 100%;
      box-sizing: border-box;
      padding: 0.75rem;
      border: 1px solid var(--gbt-hairline);
      border-radius: var(--site-border-radius);
      background: var(--bg-principal);
      color: inherit;
      font: inherit;
      text-align: left;
      cursor: pointer;

      &:hover,
      &:focus-visible {
        border-color: var(--primary);
      }
    }
    .starters__card gbt-icon {
      color: var(--primary);
    }
    .starters__name {
      font-weight: 600;
    }
    .starters__what {
      color: var(--text-secondary);
      font-size: 0.8125rem;
    }
    .starters__stages {
      margin-top: auto;
      font-family: var(--gbt-font-mono, monospace);
      font-size: 0.75rem;
      color: var(--text-secondary);
    }
  `,
})
export class PipelineStarters {
  /** The pipeline made for this repository, `null` when nothing in it was recognised. */
  prediction = input<Prediction | null>(null);
  /** The repository is being read: its proposal is on its way. */
  predicting = input(false);
  /** The default branch the proposal was read from. */
  branch = input<string | null>(null);
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
