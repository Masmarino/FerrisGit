import { NgTemplateOutlet } from '@angular/common';
import { Component, inject, input, linkedSignal, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Observable } from 'rxjs';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { GbtInput } from '@masmarino/gabarit/input';
import { SaveStatus } from '@masmarino/gabarit/save-status';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Switch } from '@masmarino/gabarit/switch';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { RepositorySettings as RepositorySettingsModel, RepositorySettingsService } from '../repository-settings.service';

type Field = 'ciEnabled' | 'pipelineFilePath' | 'requiredApprovals';
type SaveState = 'saving' | 'saved' | 'error';

@Component({
  selector: 'fg-repository-pipeline-settings',
  standalone: true,
  imports: [FormsModule, NgTemplateOutlet, Alert, Button, GbtInput, SaveStatus, Skeleton, Switch, Card, CardHeader],
  templateUrl: './repository-pipeline-settings.html',
  styleUrl: './repository-pipeline-settings.scss',
})
export class RepositoryPipelineSettings implements OnInit {
  repositoryId = input.required<string>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected readonly skeletonCards = [
    { title: '10rem', help: '70%' },
    { title: '8rem', help: '55%' },
  ];

  protected settings = signal<RepositorySettingsModel | null>(null);
  /** Saved value, then the clicked one while saving, back to the previous if the save fails. */
  protected ciEnabledShown = linkedSignal(() => this.settings()?.ciEnabled ?? false);
  protected loadState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected saveStates = signal<Partial<Record<Field, SaveState>>>({});
  protected pathError = signal<string | null>(null);
  protected approvalsError = signal<string | null>(null);

  ngOnInit(): void {
    this.refresh();
  }

  refresh(): void {
    this.loadState.set('loading');
    this.repositorySettings.get(this.repositoryId()).subscribe({
      next: (s) => {
        this.settings.set(s);
        this.loadState.set('loaded');
      },
      error: () => {
        this.loadState.set('failed');
        this.toast.show('Impossible de charger les réglages. Réessayez plus tard.', 'error');
      },
    });
  }

  setPipelineEnabled(enabled: boolean): void {
    if (!this.settings()) {
      return;
    }
    this.ciEnabledShown.set(enabled);
    this.save('ciEnabled', this.repositorySettings.update(this.repositoryId(), { ciEnabled: enabled }), 'Impossible de mettre à jour la pipeline.', () =>
      this.ciEnabledShown.set(!enabled),
    );
  }

  togglePipelineEnabled(): void {
    this.setPipelineEnabled(!this.ciEnabledShown());
  }

  /** Called on blur, not on every keystroke. */
  updatePipelineFilePath(path: string): void {
    const trimmed = path.trim();
    if (!trimmed) {
      this.pathError.set('Indiquez le chemin du fichier pipeline');
      return;
    }
    this.pathError.set(null);
    if (trimmed === this.settings()?.pipelineFilePath) {
      return;
    }
    this.save('pipelineFilePath', this.repositorySettings.update(this.repositoryId(), { pipelineFilePath: trimmed }), 'Impossible de mettre à jour le chemin de la pipeline.');
  }

  updateRequiredApprovals(value: string): void {
    const trimmed = value.trim();
    const parsed = Number(trimmed);
    if (!/^\d+$/.test(trimmed) || !Number.isSafeInteger(parsed)) {
      this.approvalsError.set('Entrez un nombre entier, 0 ou plus');
      return;
    }
    this.approvalsError.set(null);
    if (parsed === this.settings()?.requiredApprovals) {
      return;
    }
    this.save('requiredApprovals', this.repositorySettings.update(this.repositoryId(), { requiredApprovals: parsed }), "Impossible de mettre à jour le nombre d'approbations requises.");
  }

  private save(field: Field, request: Observable<RepositorySettingsModel>, errorMessage: string, onError?: () => void): void {
    this.setSaveState(field, 'saving');
    request.subscribe({
      next: (s) => {
        this.settings.set(s);
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
