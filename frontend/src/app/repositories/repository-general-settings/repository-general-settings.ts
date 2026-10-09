import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, input, linkedSignal, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { SaveStatus } from '@masmarino/gabarit/save-status';
import { SegmentedControl } from '@masmarino/gabarit/segmented-control';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Repository, RepositoriesService } from '../repositories.service';
import { RepositoryVisibility, VISIBILITY_OPTIONS } from '../repository-visibility';
import { TranslocoPipe } from '@jsverse/transloco';

type Field = 'description' | 'visibility';
type SaveState = 'saving' | 'saved' | 'error';

/**
 * The description saves on blur. Visibility asks for confirmation since it changes who can read the repo:
 * the control moves right away and snaps back if the change is cancelled or refused.
 */
@Component({
  selector: 'fg-repository-general-settings',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, NgTemplateOutlet, Alert, Button, Card, CardHeader, ConfirmDangerModal, SaveStatus, SegmentedControl, Skeleton, Textarea],
  templateUrl: './repository-general-settings.html',
  styleUrl: './repository-general-settings.scss',
})
export class RepositoryGeneralSettings implements OnInit {
  repositoryId = input.required<string>();

  private repositories = inject(RepositoriesService);
  private toast = inject(GbtToastService);

  protected readonly visibilityOptions = VISIBILITY_OPTIONS;

  protected repository = signal<Repository | null>(null);
  protected loadState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected saveStates = signal<Partial<Record<Field, SaveState>>>({});
  protected visibilityShown = linkedSignal<RepositoryVisibility>(() => this.repository()?.visibility ?? 'private');
  protected pendingVisibility = signal<RepositoryVisibility | null>(null);

  protected confirmHeading = computed(() => (this.pendingVisibility() === 'public' ? 'Rendre ce dépôt public' : 'Rendre ce dépôt privé'));
  protected confirmMessage = computed(() =>
    this.pendingVisibility() === 'public'
      ? "Il apparaîtra dans le catalogue public, et toute personne pourra lire son code, ses commits et ses releases sans compte (tant que les pages publiques de l'instance sont activées). Les personnes qui y ont déjà accès le gardent."
      : 'Il disparaîtra du catalogue public et de l\'API publique, et les visiteurs sans compte ne pourront plus le lire : leurs liens cesseront de fonctionner. Seules les personnes qui y ont accès directement ou par un groupe continueront de le voir.',
  );

  ngOnInit(): void {
    this.refresh();
  }

  refresh(): void {
    this.loadState.set('loading');
    this.repositories.getById(this.repositoryId()).subscribe({
      next: (repository) => {
        this.repository.set(repository);
        this.loadState.set('loaded');
      },
      error: () => {
        this.loadState.set('failed');
        this.toast.show('Impossible de charger les informations du dépôt. Réessayez plus tard.', 'error');
      },
    });
  }

  /** Called on blur, not on every keystroke. */
  updateDescription(value: string): void {
    const description = value.trim();
    if (description === this.repository()?.description) {
      return;
    }
    this.save('description', { description }, "Impossible de mettre à jour la description.");
  }

  protected askVisibility(visibility: RepositoryVisibility): void {
    if (visibility === this.repository()?.visibility) {
      return;
    }
    this.visibilityShown.set(visibility);
    this.pendingVisibility.set(visibility);
  }

  protected confirmVisibility(): void {
    const visibility = this.pendingVisibility();
    this.pendingVisibility.set(null);
    if (!visibility) {
      return;
    }
    this.save('visibility', { visibility }, 'Impossible de changer la visibilité du dépôt.', () => this.visibilityShown.set(this.repository()?.visibility ?? 'private'));
  }

  protected cancelVisibility(): void {
    this.pendingVisibility.set(null);
    this.visibilityShown.set(this.repository()?.visibility ?? 'private');
  }

  private save(field: Field, update: { description?: string; visibility?: RepositoryVisibility }, errorMessage: string, onError?: () => void): void {
    this.setSaveState(field, 'saving');
    this.repositories.update(this.repositoryId(), update).subscribe({
      next: (repository) => {
        this.repository.set(repository);
        this.setSaveState(field, 'saved');
      },
      error: () => {
        onError?.();
        this.setSaveState(field, 'error');
        this.toast.show(errorMessage, 'error');
      },
    });
  }

  private setSaveState(field: Field, state: SaveState): void {
    this.saveStates.update((states) => ({ ...states, [field]: state }));
  }
}
