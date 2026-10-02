import { Component, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert, Badge, Button, Card, ConfirmDangerModal, EmptyState, GbtInput, Icon, ListRow, SkeletonList, GbtToastService } from '@masmarino/gabarit';
import { CiVariableSummary, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';

@Component({
  selector: 'fg-repository-ci-variables',
  standalone: true,
  imports: [FormsModule, GbtInput, Badge, Alert, EmptyState, Button, ConfirmDangerModal, Icon, SkeletonList, ListRow, Card],
  templateUrl: './repository-ci-variables.html',
  styleUrl: './repository-ci-variables.scss',
})
export class RepositoryCiVariables implements OnInit {
  repositoryId = input.required<string>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected list = createSettingsList(() => this.repositorySettings.listCiVariables(this.repositoryId()));
  protected newVariableKey = signal('');
  protected newVariableValue = signal('');
  protected variablePendingDelete = signal<CiVariableSummary | null>(null);
  /** True while the confirmed deletion runs: the confirmation stays open and ignores a second click. */
  protected deleting = signal(false);

  ngOnInit(): void {
    this.list.refresh();
  }

  addVariable(): void {
    const key = this.newVariableKey().trim();
    const value = this.newVariableValue();
    if (!key || !value) {
      return;
    }
    this.repositorySettings.setCiVariable(this.repositoryId(), key, value, true).subscribe({
      next: () => {
        this.newVariableKey.set('');
        this.newVariableValue.set('');
        this.list.refresh();
        this.toast.show('Variable ajoutée.');
      },
      error: () => this.toast.show("Impossible d'ajouter la variable.", 'error'),
    });
  }

  confirmDeleteVariable(variable: CiVariableSummary): void {
    this.variablePendingDelete.set(variable);
  }

  protected deleteVariable(id: string): void {
    if (this.deleting()) {
      return;
    }
    this.deleting.set(true);
    this.repositorySettings.deleteCiVariable(this.repositoryId(), id).subscribe({
      next: () => {
        this.deleting.set(false);
        this.variablePendingDelete.set(null);
        this.list.refresh();
        this.toast.show('Variable supprimée.');
      },
      // The confirmation covers the page, so an error must close it first to be visible.
      error: () => {
        this.deleting.set(false);
        this.variablePendingDelete.set(null);
        this.toast.show('Impossible de supprimer la variable.', 'error');
      },
    });
  }
}
