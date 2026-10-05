import { Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { DatePicker } from '@masmarino/gabarit/date-picker';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { createSettingsList } from '../settings-list';

// A due date is a calendar day stored as UTC midnight. Formatting in local time would show the
// previous day west of UTC.
const DUE_DATE_FORMAT = new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' });

@Component({
  selector: 'fg-repository-milestones-settings',
  standalone: true,
  imports: [FormsModule, GbtInput, Badge, Alert, EmptyState, Button, DatePicker, ConfirmDangerModal, Icon, SkeletonList, ListRow, Card],
  templateUrl: './repository-milestones-settings.html',
  styleUrl: './repository-milestones-settings.scss',
})
export class RepositoryMilestonesSettings implements OnInit {
  repositoryId = input.required<string>();

  private milestonesService = inject(MilestonesService);
  private toast = inject(GbtToastService);

  protected list = createSettingsList(() => this.milestonesService.listForRepository(this.repositoryId()));
  protected newMilestoneTitle = signal('');
  protected newMilestoneDueDate = signal<Date | null>(null);
  protected milestonePendingDelete = signal<Milestone | null>(null);

  protected rows = computed(() => {
    const today = this.todayUtc();
    return this.list.items().map((milestone) => {
      const due = milestone.dueDate ? new Date(milestone.dueDate) : null;
      const validDue = due && !Number.isNaN(due.getTime()) ? due : null;
      return {
        milestone,
        due: validDue ? `Échéance le ${DUE_DATE_FORMAT.format(validDue)}` : 'Sans échéance',
        late: milestone.state === 'open' && validDue !== null && validDue.getTime() < today,
      };
    });
  });

  ngOnInit(): void {
    this.list.refresh();
  }

  private todayUtc(): number {
    const now = new Date();
    return Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  }

  // The picker returns local midnight, and toISOString() on that would shift the day east of UTC, so
  // rebuild the same Y/M/D at UTC midnight.
  protected dueDateForApi(picked: Date | null): string | null {
    if (!picked) {
      return null;
    }
    return new Date(Date.UTC(picked.getFullYear(), picked.getMonth(), picked.getDate())).toISOString();
  }

  addMilestone(): void {
    const title = this.newMilestoneTitle().trim();
    if (!title) {
      return;
    }
    // This section has no description field, so it's created empty.
    this.milestonesService.create({ repositoryId: this.repositoryId() }, title, '', this.dueDateForApi(this.newMilestoneDueDate())).subscribe({
      next: () => {
        this.newMilestoneTitle.set('');
        this.newMilestoneDueDate.set(null);
        this.list.refresh();
        this.toast.show('Milestone créé.');
      },
      error: () => this.toast.show('Impossible de créer ce milestone.', 'error'),
    });
  }

  confirmDeleteMilestone(milestone: Milestone): void {
    this.milestonePendingDelete.set(milestone);
  }

  deleteMilestone(): void {
    const milestone = this.milestonePendingDelete();
    if (!milestone) {
      return;
    }
    this.milestonesService.delete(milestone.id).subscribe({
      next: () => {
        this.milestonePendingDelete.set(null);
        this.list.refresh();
        this.toast.show('Milestone supprimé.');
      },
      error: () => {
        this.milestonePendingDelete.set(null);
        this.toast.show('Impossible de supprimer ce milestone.', 'error');
      },
    });
  }
}
