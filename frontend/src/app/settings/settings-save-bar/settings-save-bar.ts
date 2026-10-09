import { Component, input, output } from '@angular/core';
import { Button } from '@masmarino/gabarit/button';
import { Icon } from '@masmarino/gabarit/icon';
import { TranslocoPipe } from '@jsverse/transloco';

/**
 * The end of a settings section saved with one button: whether something is left to save, "Annuler les
 * modifications", and "Enregistrer", the submit button of the section's form. A save is confirmed by a toast, so
 * nothing stays here once saved.
 */
@Component({
  selector: 'fg-settings-save-bar',
  standalone: true,
  imports: [TranslocoPipe, Button, Icon],
  template: `
    <!-- Always in the DOM: a live region only announces changes if it already exists. Empty once saved: the toast says so. -->
    <p class="settings-save-bar__summary" role="status">
      @if (dirty()) {
        <gbt-icon name="circle-dot" aria-hidden="true" />{{ 'common.unsavedChanges' | transloco }}
      }
    </p>
    @if (dirty()) {
      <gbt-button variant="secondary" [text]="'common.discardChanges' | transloco" [disabled]="saving()" (clicked)="discard.emit()" />
    }
    <gbt-button type="submit" [text]="'common.save' | transloco" [loadingLabel]="'common.saving' | transloco" [loading]="saving()" [disabled]="saving() || !dirty()" />
  `,
  styleUrl: './settings-save-bar.scss',
})
export class SettingsSaveBar {
  dirty = input.required<boolean>();
  saving = input.required<boolean>();
  discard = output<void>();
}
