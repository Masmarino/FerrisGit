import { Component, computed, inject, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert, Button, GbtInput, Modal, Textarea, GbtToastService } from '@masmarino/gabarit';
import { groupCreationError, GroupsService } from '../groups.service';

const NAME_REQUIRED = 'Le nom est requis';

/**
 * The "Nouveau groupe" dialog, for a group at the root. Any signed-in user can create one; the server only refuses a
 * name already used by a user account or another root group. The parent renders it under `@if`, so it's rebuilt on each
 * open and never keeps an old draft. A failed creation leaves it open with the draft.
 */
@Component({
  selector: 'fg-create-group-modal',
  standalone: true,
  imports: [FormsModule, Modal, Alert, GbtInput, Button, Textarea],
  templateUrl: './create-group-modal.html',
  styleUrl: './create-group-modal.scss',
})
export class CreateGroupModal {
  private groups = inject(GroupsService);
  private toast = inject(GbtToastService);

  close = output<void>();
  created = output<void>();

  // Public so the spec can call them.
  name = signal('');
  description = signal('');
  error = signal('');
  creating = signal(false);

  protected nameError = computed(() => (this.error() === NAME_REQUIRED ? NAME_REQUIRED : null));
  protected formError = computed(() => (this.error() && this.error() !== NAME_REQUIRED ? this.error() : null));

  protected onNameChange(value: string): void {
    this.name.set(value);
    if (this.error() === NAME_REQUIRED && value.trim()) {
      this.error.set('');
    }
  }

  submit(): void {
    if (this.creating()) {
      return;
    }
    const name = this.name().trim();
    if (!name) {
      this.error.set(NAME_REQUIRED);
      return;
    }
    this.error.set('');
    this.creating.set(true);
    this.groups.createRoot(name, this.description().trim()).subscribe({
      next: () => {
        this.creating.set(false);
        this.toast.show('Groupe créé.');
        this.created.emit();
      },
      error: (err: { status?: number }) => {
        this.creating.set(false);
        this.error.set(groupCreationError(err.status, 'le groupe'));
      },
    });
  }
}
