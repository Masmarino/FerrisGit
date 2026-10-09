import { Component, input, output, signal } from '@angular/core';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { RepositoryCiVariables } from '../../repositories/repository-ci-variables/repository-ci-variables';
import { HelpTip } from './help-tip';
import { HELP } from './pipeline-help';

/** A secret name and the jobs that read it. */
export interface SecretUse {
  name: string;
  jobs: string[];
}

/**
 * The repository's secrets, next to the pipeline that reads them: the ones the jobs expect but are missing, the ones
 * nothing reads, and the form to create them. A value is never shown again once saved.
 */
@Component({
  selector: 'fg-pipeline-secrets',
  standalone: true,
  imports: [Alert, Badge, Button, HelpTip, RepositoryCiVariables],
  templateUrl: './pipeline-secrets.html',
  styleUrl: './pipeline-secrets.scss',
})
export class PipelineSecrets {
  repositoryId = input.required<string>();
  /** Only a maintainer can read or write the repository's secrets. */
  canManage = input(false);
  engine = input<string | null>(null);
  /** Read by the jobs, but missing from the repository. */
  missing = input<SecretUse[]>([]);
  /** In the repository, with the jobs that read them. */
  existing = input<SecretUse[]>([]);
  /** A name to start the form with, set from outside (a job asking for a secret it lacks). */
  suggestion = input<string | null>(null);

  changed = output<void>();

  protected readonly help = HELP;
  protected readonly chosen = signal<string | null>(null);
}
