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
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

@Component({
  selector: 'fg-repository-ci-variables',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, GbtInput, Badge, Alert, EmptyState, Button, ConfirmDangerModal, Icon, SkeletonList, ListRow, Card],
  templateUrl: './repository-ci-variables.html',
  styleUrl: './repository-ci-variables.scss',
})
export class RepositoryCiVariables implements OnInit {
  repositoryId = input.required<string>();
  /** A name to start the new variable with, when another part of the page already knows which one is needed. */
  prefillKey = input<string | null>(null);
  /** Emitted when the list changes: a variable was added, replaced or deleted. */
  changed = output<void>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected list = createSettingsList(() => this.repositorySettings.listCiVariables(this.repositoryId()));
  protected newVariableKey = signal('');
  protected newVariableValue = signal('');
  /** The name has to be a valid environment variable name, or the job would never see it. */
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
        this.toast.show(t('repositories.variables.added'));
      },
      error: () => this.toast.show(t('repositories.variables.addFailed'), 'error'),
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
        this.toast.show(t('repositories.variables.deleted'));
      },
      // Close the confirmation first, it covers the page and would hide the error.
      error: () => {
        this.deleting.set(false);
        this.variablePendingDelete.set(null);
        this.toast.show(t('repositories.variables.deleteFailed'), 'error');
      },
    });
  }
}
