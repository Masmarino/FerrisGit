import { Component, output } from '@angular/core';
import { Icon } from '@masmarino/gabarit/icon';
import { HelpTip } from './help-tip';
import { HELP } from './pipeline-help';
import { PIPELINE_TEMPLATES, PipelineTemplate } from './pipeline-catalog';

/** Whole pipelines to start from, for an empty one: a click lays out the stages and the jobs. */
@Component({
  selector: 'fg-pipeline-starters',
  standalone: true,
  imports: [Icon, HelpTip],
  template: `
    <section class="starters" aria-labelledby="starters-title">
      <div class="starters__head">
        <h2 id="starters-title" class="starters__title">Partir d'un modèle</h2>
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
      padding: 1rem;
      border: 1px dashed var(--gbt-hairline);
      border-radius: var(--site-border-radius);
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
  chosen = output<PipelineTemplate>();
  protected readonly help = HELP;
  protected readonly templates = PIPELINE_TEMPLATES;
}
