import { Component, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Tag } from '@masmarino/gabarit/tag';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Label, LabelsService } from '../../labels/labels.service';
import { createSettingsList } from '../settings-list';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

const LABEL_PALETTE: { color: string; name: string }[] = [
  {
    color: '#dc2626',
    get name() {
      return t('repositories.colors.red');
    },
  },
  {
    color: '#ea580c',
    get name() {
      return t('repositories.colors.orange');
    },
  },
  {
    color: '#ca8a04',
    get name() {
      return t('repositories.colors.yellow');
    },
  },
  {
    color: '#16a34a',
    get name() {
      return t('repositories.colors.green');
    },
  },
  {
    color: '#0d9488',
    get name() {
      return t('repositories.colors.teal');
    },
  },
  {
    color: '#2563eb',
    get name() {
      return t('repositories.colors.blue');
    },
  },
  {
    color: '#4f46e5',
    get name() {
      return t('repositories.colors.indigo');
    },
  },
  {
    color: '#7c3aed',
    get name() {
      return t('repositories.colors.violet');
    },
  },
  {
    color: '#c026d3',
    get name() {
      return t('repositories.colors.fuchsia');
    },
  },
  {
    color: '#db2777',
    get name() {
      return t('repositories.colors.pink');
    },
  },
  {
    color: '#57534e',
    get name() {
      return t('repositories.colors.brown');
    },
  },
  {
    color: '#6b7280',
    get name() {
      return t('repositories.colors.grey');
    },
  },
];

let nextId = 0;

@Component({
  selector: 'fg-repository-labels-settings',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, GbtInput, Alert, EmptyState, Button, Tag, ConfirmDangerModal, Skeleton, ListRow, Card],
  templateUrl: './repository-labels-settings.html',
  styleUrl: './repository-labels-settings.scss',
})
export class RepositoryLabelsSettings implements OnInit {
  repositoryId = input.required<string>();

  private labelsService = inject(LabelsService);
  private toast = inject(GbtToastService);

  protected readonly labelPalette = LABEL_PALETTE;
  protected readonly skeletonRows = ['5rem', '7.5rem', '4rem'];
  protected readonly paletteName = `label-color-${nextId++}`;

  protected list = createSettingsList(() => this.labelsService.listForRepository(this.repositoryId()));
  protected newLabelName = signal('');
  protected newLabelColor = signal(LABEL_PALETTE[0].color);
  protected labelPendingDelete = signal<Label | null>(null);

  ngOnInit(): void {
    this.list.refresh();
  }

  addLabel(): void {
    const name = this.newLabelName().trim();
    if (!name) {
      return;
    }
    this.labelsService.create({ repositoryId: this.repositoryId() }, name, this.newLabelColor()).subscribe({
      next: () => {
        this.newLabelName.set('');
        this.list.refresh();
        this.toast.show(t('repositories.labels.added'));
      },
      error: () => this.toast.show(t('repositories.labels.createFailed'), 'error'),
    });
  }

  confirmDeleteLabel(label: Label): void {
    this.labelPendingDelete.set(label);
  }

  deleteLabel(): void {
    const label = this.labelPendingDelete();
    if (!label) {
      return;
    }
    this.labelsService.delete(label.id).subscribe({
      next: () => {
        this.labelPendingDelete.set(null);
        this.list.refresh();
        this.toast.show(t('repositories.labels.deleted'));
      },
      // Close the modal first, it covers the card and would hide the error.
      error: () => {
        this.labelPendingDelete.set(null);
        this.toast.show(t('repositories.labels.deleteFailed'), 'error');
      },
    });
  }
}
