import { Component, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { CollaboratorSummary, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';
import { TranslocoPipe } from '@jsverse/transloco';

type Role = CollaboratorSummary['role'];

@Component({
  selector: 'fg-repository-collaborators-settings',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, GbtInput, Select, Alert, EmptyState, Button, ConfirmDangerModal, SkeletonList, ListRow, UserChip, Card, GbtRelativeTimePipe, GbtDateTimePipe],
  templateUrl: './repository-collaborators-settings.html',
  styleUrl: './repository-collaborators-settings.scss',
})
export class RepositoryCollaboratorsSettings implements OnInit {
  repositoryId = input.required<string>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected list = createSettingsList(() => this.repositorySettings.listCollaborators(this.repositoryId()));
  protected newCollaboratorUsername = signal('');
  protected newCollaboratorRole = signal<Role>('contributor');
  protected collaboratorPendingRemoval = signal<CollaboratorSummary | null>(null);
  /** Keeps the confirmation open and inert while the removal request runs. */
  protected removing = signal(false);
  /** Role picked per row (by user id) while it saves. Dropping the entry on failure makes NgModel put the old role back in the select. */
  protected pendingRoles = signal<ReadonlyMap<string, Role>>(new Map());
  protected readonly roleOptions: SelectOption<Role>[] = [
    { value: 'reader', label: 'Lecteur' },
    { value: 'contributor', label: 'Contributeur' },
    { value: 'maintainer', label: 'Mainteneur' },
  ];

  ngOnInit(): void {
    this.list.refresh();
  }

  addCollaborator(): void {
    const username = this.newCollaboratorUsername().trim();
    if (!username) {
      return;
    }
    this.repositorySettings.addCollaborator(this.repositoryId(), username, this.newCollaboratorRole()).subscribe({
      next: () => {
        this.newCollaboratorUsername.set('');
        this.list.refresh();
        this.toast.show('Collaborateur ajouté.');
      },
      error: () => this.toast.show("Impossible d'ajouter ce collaborateur (nom d'utilisateur inconnu, ou vous n'êtes pas le propriétaire de ce dépôt).", 'error'),
    });
  }

  confirmRemoveCollaborator(collaborator: CollaboratorSummary): void {
    this.collaboratorPendingRemoval.set(collaborator);
  }

  protected removeCollaborator(username: string): void {
    if (this.removing()) {
      return;
    }
    this.removing.set(true);
    this.repositorySettings.removeCollaborator(this.repositoryId(), username).subscribe({
      next: () => {
        this.removing.set(false);
        this.collaboratorPendingRemoval.set(null);
        this.list.refresh();
        this.toast.show('Collaborateur retiré.');
      },
      // Close the confirmation first, it covers the page and would hide the error.
      error: () => {
        this.removing.set(false);
        this.collaboratorPendingRemoval.set(null);
        this.toast.show("Impossible de retirer ce collaborateur (vous n'êtes peut-être pas le propriétaire de ce dépôt).", 'error');
      },
    });
  }

  protected changeCollaboratorRole(collaborator: CollaboratorSummary, role: Role): void {
    const { userId, username } = collaborator;
    this.setPendingRole(userId, role);
    this.repositorySettings.setCollaboratorRole(this.repositoryId(), username, role).subscribe({
      next: () => {
        this.list.items.update((list) => list.map((c) => (c.userId === userId ? { ...c, role } : c)));
        this.setPendingRole(userId, null);
        this.toast.show('Rôle mis à jour.');
      },
      error: () => {
        this.setPendingRole(userId, null);
        this.toast.show("Impossible de modifier le rôle de ce collaborateur (vous n'êtes peut-être pas mainteneur de ce dépôt).", 'error');
      },
    });
  }

  private setPendingRole(userId: string, role: Role | null): void {
    this.pendingRoles.update((current) => {
      const next = new Map(current);
      if (role === null) {
        next.delete(userId);
      } else {
        next.set(userId, role);
      }
      return next;
    });
  }
}
