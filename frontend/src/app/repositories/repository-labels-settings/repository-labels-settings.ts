import { Component, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert, Button, Card, ConfirmDangerModal, EmptyState, GbtInput, ListRow, Skeleton, Tag, GbtToastService } from '@masmarino/gabarit';
import { Label, LabelsService } from '../../labels/labels.service';
import { createSettingsList } from '../settings-list';

const LABEL_PALETTE: { color: string; name: string }[] = [
  { color: '#dc2626', name: 'Rouge' },
  { color: '#ea580c', name: 'Orange' },
  { color: '#ca8a04', name: 'Jaune' },
  { color: '#16a34a', name: 'Vert' },
  { color: '#0d9488', name: 'Sarcelle' },
  { color: '#2563eb', name: 'Bleu' },
  { color: '#4f46e5', name: 'Indigo' },
  { color: '#7c3aed', name: 'Violet' },
  { color: '#c026d3', name: 'Fuchsia' },
  { color: '#db2777', name: 'Rose' },
  { color: '#57534e', name: 'Brun' },
  { color: '#6b7280', name: 'Gris' },
];

let nextId = 0;

@Component({
  selector: 'fg-repository-labels-settings',
  standalone: true,
  imports: [FormsModule, GbtInput, Alert, EmptyState, Button, Tag, ConfirmDangerModal, Skeleton, ListRow, Card],
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
        this.toast.show('Label ajouté.');
      },
      error: () => this.toast.show('Impossible de créer ce label.', 'error'),
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
        this.toast.show('Label supprimé.');
      },
      // Close the modal first, it covers the card and would hide the error.
      error: () => {
        this.labelPendingDelete.set(null);
        this.toast.show('Impossible de supprimer ce label.', 'error');
      },
    });
  }
}
