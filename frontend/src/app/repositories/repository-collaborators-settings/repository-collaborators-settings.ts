import { Component, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert, Button, Card, ConfirmDangerModal, EmptyState, GbtDateTimePipe, GbtInput, GbtRelativeTimePipe, ListRow, Select, SelectOption, SkeletonList, UserChip, GbtToastService } from '@masmarino/gabarit';
import { CollaboratorSummary, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';

type Role = CollaboratorSummary['role'];

@Component({
  selector: 'fg-repository-collaborators-settings',
  standalone: true,
  imports: [FormsModule, GbtInput, Select, Alert, EmptyState, Button, ConfirmDangerModal, SkeletonList, ListRow, UserChip, Card, GbtRelativeTimePipe, GbtDateTimePipe],
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
  /** True while the confirmed removal runs: the confirmation stays open and ignores a second click. */
  protected removing = signal(false);
  /** The role picked in each row while the change saves, keyed by user id. Dropping it on failure makes `NgModel` write the previous role back into the select. */
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
      // The confirmation covers the page, so an error must close it first to be visible.
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
