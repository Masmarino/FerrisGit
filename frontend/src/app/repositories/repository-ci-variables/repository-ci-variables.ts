import { Component, computed, effect, inject, input, OnInit, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { CiVariableSummary, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';
import { envNameProblem } from '../ci-variable-name';

@Component({
  selector: 'fg-repository-ci-variables',
  standalone: true,
  imports: [FormsModule, GbtInput, Badge, Alert, EmptyState, Button, ConfirmDangerModal, Icon, SkeletonList, ListRow, Card],
  templateUrl: './repository-ci-variables.html',
  styleUrl: './repository-ci-variables.scss',
})
export class RepositoryCiVariables implements OnInit {
  repositoryId = input.required<string>();
  /** A name to start the new variable with, when something else already knows which one is wanted. */
  prefillKey = input<string | null>(null);
  /** The list changed: a variable was added, replaced or deleted. */
  changed = output<void>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected list = createSettingsList(() => this.repositorySettings.listCiVariables(this.repositoryId()));
  protected newVariableKey = signal('');
  protected newVariableValue = signal('');
  /** The name has to be one an environment variable can have, or the job would never see it. */
  protected keyProblem = computed(() => envNameProblem(this.newVariableKey().trim()));
  protected variablePendingDelete = signal<CiVariableSummary | null>(null);
  /** Keeps the confirmation open and inert while the delete request runs. */
  protected deleting = signal(false);

  constructor() {
    effect(() => {
      const key = this.prefillKey();
      if (key) {
        this.newVariableKey.set(key);
      }
    });
  }

  ngOnInit(): void {
    this.list.refresh();
  }

  addVariable(): void {
    const key = this.newVariableKey().trim();
    const value = this.newVariableValue();
    if (!key || !value || this.keyProblem()) {
      return;
    }
    this.repositorySettings.setCiVariable(this.repositoryId(), key, value, true).subscribe({
      next: () => {
        this.newVariableKey.set('');
        this.newVariableValue.set('');
        this.list.refresh();
        this.changed.emit();
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
        this.changed.emit();
        this.toast.show('Variable supprimée.');
      },
      // Close the confirmation first, it covers the page and would hide the error.
      error: () => {
        this.deleting.set(false);
        this.variablePendingDelete.set(null);
        this.toast.show('Impossible de supprimer la variable.', 'error');
      },
    });
  }
}
