import { Component, input } from '@angular/core';
import { TranslocoPipe } from '@jsverse/transloco';
import { Button } from '@masmarino/gabarit/button';
import { Popover } from '@masmarino/gabarit/popover';
import { HelpText } from './pipeline-help';

/**
 * A small "?" that opens an explanation. It opens on click or tap, so that it also works on a phone, where a hover
 * bubble would not. Escape or a click elsewhere closes it.
 */
@Component({
  selector: 'fg-help-tip',
  standalone: true,
  imports: [TranslocoPipe, Button, Popover],
  template: `
    <gbt-popover [align]="align()" #bubble="gbtPopover" class="help-tip">
      <gbt-button
        variant="ghost"
        size="small"
        [iconOnly]="true"
        iconName="circle-help"
        [ariaLabel]="'pipelines.editor.helpFor' | transloco: { title: help().title }"
        [ariaHaspopup]="true"
        [ariaExpanded]="bubble.open()"
        [ariaControls]="bubble.panelId"
      />
      <div popover-content class="help-tip__panel">
        <p class="help-tip__title">{{ help().title }}</p>
        <p class="help-tip__body">{{ help().body }}</p>
      </div>
    </gbt-popover>
  `,
  styles: `
    :host {
      display: inline-flex;
      vertical-align: middle;
    }
    .help-tip__panel {
      box-sizing: border-box;
      width: min(20rem, calc(100vw - 2rem));
      margin: -0.25rem;
      padding: 0;
      font-size: 0.8125rem;
      line-height: 1.45;
      color: var(--text-primary);
      white-space: normal;
    }
    .help-tip__title {
      margin: 0 0 0.25rem;
      font-weight: 600;
    }
    .help-tip__body {
      margin: 0;
      color: var(--text-secondary);
      font-weight: 400;
    }
  `,
})
export class HelpTip {
  help = input.required<HelpText>();
  align = input<'start' | 'end'>('start');
}
