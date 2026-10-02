import { Component, inject, input } from '@angular/core';
import { SaveStatus } from '@masmarino/gabarit';
import { SettingsEditor, SettingsField } from './settings-editor';

/** A field's save state. A live region that stays in the DOM, so each change is announced. */
@Component({
  selector: 'fg-field-save-state',
  standalone: true,
  imports: [SaveStatus],
  template: `<gbt-save-status [state]="editor.saveStates()[field()] ?? 'idle'" savingLabel="Enregistrement…" savedLabel="Enregistré" errorLabel="Non enregistré" />`,
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
