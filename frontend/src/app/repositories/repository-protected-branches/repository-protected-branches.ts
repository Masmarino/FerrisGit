import { Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { HttpErrorResponse } from '@angular/common/http';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { TranslocoPipe } from '@jsverse/transloco';
import { ProtectedBranchRule, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';
import { branchPatternProblem } from '../branch-pattern';
import { t } from '../../shared/i18n/translator';

@Component({
  selector: 'fg-repository-protected-branches',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, GbtInput, Alert, EmptyState, Button, ConfirmDangerModal, Icon, SkeletonList, ListRow, Card],
  templateUrl: './repository-protected-branches.html',
  styleUrl: './repository-protected-branches.scss',
})
export class RepositoryProtectedBranches implements OnInit {
  repositoryId = input.required<string>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected list = createSettingsList(() => this.repositorySettings.listProtectedBranches(this.repositoryId()));
  protected newPattern = signal('');
  /** The same rules as the server's, so that a pattern it would refuse is pointed out before sending. */
  protected patternProblem = computed(() => branchPatternProblem(this.newPattern().trim()));
  protected rulePendingDelete = signal<ProtectedBranchRule | null>(null);
  /** Keeps the confirmation open and inert while the delete request runs. */
  protected deleting = signal(false);

  ngOnInit(): void {
    this.list.refresh();
  }

  protect(): void {
    const pattern = this.newPattern().trim();
    if (!pattern || this.patternProblem()) {
      return;
    }
    this.repositorySettings.protectBranch(this.repositoryId(), pattern).subscribe({
      next: () => {
        this.newPattern.set('');
        this.list.refresh();
        this.toast.show(t('repositories.protectedBranches.added'));
      },
      error: (error: HttpErrorResponse) =>
        this.toast.show(t(error.status === 409 ? 'repositories.protectedBranches.alreadyProtected' : 'repositories.protectedBranches.addFailed'), 'error'),
    });
  }

  confirmDelete(rule: ProtectedBranchRule): void {
    this.rulePendingDelete.set(rule);
  }

  protected unprotect(id: string): void {
    if (this.deleting()) {
      return;
    }
    this.deleting.set(true);
    this.repositorySettings.unprotectBranch(this.repositoryId(), id).subscribe({
      next: () => {
        this.deleting.set(false);
        this.rulePendingDelete.set(null);
        this.list.refresh();
        this.toast.show(t('repositories.protectedBranches.deleted'));
      },
      // Close the confirmation first, it covers the page and would hide the error.
      error: () => {
        this.deleting.set(false);
        this.rulePendingDelete.set(null);
        this.toast.show(t('repositories.protectedBranches.deleteFailed'), 'error');
      },
    });
  }
}
