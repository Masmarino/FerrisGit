import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { JobStatus } from '@masmarino/gabarit'
import { Glow } from '../glow.directive'
import { Reveal } from '../motion/reveal.directive'
import { PlanHighlight, PlanPoint } from '../plan-highlight'

/**
 * What FerrisGit does, as tiles that each show a piece of the interface. The facts come from the docs
 * (docs/utilisation). Seven tiles carry the number of the architecture-plan circle that leads to them (ids d1 to d7).
 */
@Component({
  selector: 'app-capabilities',
  imports: [TranslocoPipe, JobStatus, Glow, PlanPoint, Reveal],
  templateUrl: './capabilities.html',
  styleUrl: './capabilities.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Capabilities {
  protected readonly plan = inject(PlanHighlight)

  /** Whether the suggestion of the review tile has been applied. */
  protected readonly applied = signal(false)

  // Reader and contributor columns per action (docs/utilisation/roles-et-permissions.md). A maintainer can do all.
  protected readonly roleRows: { id: string; reader: boolean; contributor: boolean }[] = [
    { id: 'read', reader: true, contributor: true },
    { id: 'write', reader: false, contributor: true },
    { id: 'merge', reader: false, contributor: false },
  ]

  protected readonly searchGroups = ['repos', 'issues', 'mrs'] as const

  protected apply(): void {
    this.applied.set(true)
  }

  protected reset(): void {
    this.applied.set(false)
  }
}
