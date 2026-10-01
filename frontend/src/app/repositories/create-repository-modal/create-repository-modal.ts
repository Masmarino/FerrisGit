import { Component, computed, inject, input, output, signal, OnInit } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RepositoriesService } from '../repositories.service';
import { GroupsService, WritableGroup } from '../../groups/groups.service';
import { Alert, Button, GbtInput, Icon, Modal, SegmentedControl, SegmentedControlOption, Select, SelectOption, Switch, Textarea, GbtToastService } from '@masmarino/gabarit';

const NAME_REQUIRED = 'Le nom est requis';

const VISIBILITY_OPTIONS: SegmentedControlOption<'private' | 'public'>[] = [
  { value: 'private', label: 'Privé' },
  { value: 'public', label: 'Public' },
];

/**
 * The "Nouveau dépôt" dialog. The parent renders it under `@if`, so it is rebuilt each time it opens and
 * never keeps an old draft. A failed creation leaves it open with the draft.
 */
@Component({
  selector: 'fg-create-repository-modal',
  standalone: true,
  imports: [FormsModule, Modal, Alert, GbtInput, Button, Icon, SegmentedControl, Switch, Select, Textarea],
  templateUrl: './create-repository-modal.html',
  styleUrl: './create-repository-modal.scss',
})
export class CreateRepositoryModal implements OnInit {
  private repositories = inject(RepositoriesService);
  private groups = inject(GroupsService);
  private toast = inject(GbtToastService);

  defaultLocation = input('');

  close = output<void>();
  created = output<void>();

  // Not `protected` because the spec calls it directly.
  name = signal('');
  protected description = signal('');
  error = signal('');
  protected isPublic = signal(false);
  protected advancedOpen = signal(false);
  protected creating = signal(false);
  protected writableGroups = signal<WritableGroup[]>([]);
  location = signal<string>('');
  protected locationOptions = computed<SelectOption<string>[]>(() => [
    { value: '', label: 'Personnel' },
    ...this.writableGroups().map((g) => ({ value: g.path, label: g.path })),
  ]);
  protected readonly visibilityOptions = VISIBILITY_OPTIONS;
  protected visibility = computed<'private' | 'public'>(() => (this.isPublic() ? 'public' : 'private'));

  protected nameError = computed(() => (this.error() === NAME_REQUIRED ? NAME_REQUIRED : null));
  protected formError = computed(() => (this.error() && this.error() !== NAME_REQUIRED ? this.error() : null));

  protected ciEnabled = signal(true);
  requiredApprovals = signal('0');
  pipelineFilePath = signal('.ferrisgit-ci.yml');

  ngOnInit(): void {
    this.groups.listWritable().subscribe({
      next: (g) => {
        this.writableGroups.set(g);
        const wanted = this.defaultLocation();
        if (wanted && this.location() === '' && g.some((group) => group.path === wanted)) {
          this.location.set(wanted);
        }
      },
      error: () => {},
    });
  }

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
    if (!this.name().trim()) {
      this.error.set(NAME_REQUIRED);
      return;
    }
    const visibility = this.isPublic() ? 'public' : 'private';
    const requiredApprovals = Number(this.requiredApprovals());
    this.error.set('');
    this.creating.set(true);
    this.repositories
      .create(this.name().trim(), visibility, {
        description: this.description().trim(),
        ciEnabled: this.ciEnabled(),
        requiredApprovals: Number.isFinite(requiredApprovals) ? requiredApprovals : 0,
        pipelineFilePath: this.pipelineFilePath().trim() || '.ferrisgit-ci.yml',
        groupPath: this.location() || undefined,
      })
      .subscribe({
        next: () => {
          this.creating.set(false);
          this.toast.show('Dépôt créé.');
          this.created.emit();
        },
        error: (err: { status?: number }) => {
          this.creating.set(false);
          if (err.status === 409) {
            this.error.set('Ce nom est déjà utilisé');
          } else if (err.status === 400) {
            this.error.set('Nom invalide : lettres, chiffres, - et _ uniquement');
          } else {
            this.error.set('Impossible de créer le dépôt. Réessayez plus tard.');
          }
        },
      });
  }
}
