import { Component, inject, input } from '@angular/core';
import { SaveStatus } from '@masmarino/gabarit/save-status';
import { SettingsEditor, SettingsField } from './settings-editor';
import { TranslocoPipe } from '@jsverse/transloco';

/** A field's save state. A live region that stays in the DOM, so each change is announced. */
@Component({
  selector: 'fg-field-save-state',
  standalone: true,
  imports: [TranslocoPipe, SaveStatus],
  template: `<gbt-save-status [state]="editor.saveStates()[field()] ?? 'idle'" [savingLabel]="'common.savingShort' | transloco" [savedLabel]="'common.saved' | transloco" [errorLabel]="'common.notSaved' | transloco" />`,
  styles: `
    :host {
      display: contents;
    }

    gbt-save-status {
      flex: none;
    }
  `,
})
export class FieldSaveState {
  protected editor = inject(SettingsEditor);
  field = input.required<SettingsField>();
}
