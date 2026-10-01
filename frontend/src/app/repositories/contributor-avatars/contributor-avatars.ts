import { Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { Skeleton, UserChip } from '@masmarino/gabarit';
import { Contributor, RepositoriesService } from '../repositories.service';

const MAX_VISIBLE_CONTRIBUTORS = 8;

@Component({
  selector: 'fg-contributor-avatars',
  standalone: true,
  imports: [Skeleton, UserChip],
  templateUrl: './contributor-avatars.html',
  styleUrl: './contributor-avatars.scss',
})
export class ContributorAvatars implements OnInit {
  repositoryId = input.required<string>();
  ref = input.required<string>();

  private repositories = inject(RepositoriesService);
  /** `null` while loading. A failed request shows as "no contributors". */
  protected contributors = signal<Contributor[] | null>(null);

  protected visible = computed(() => (this.contributors() ?? []).slice(0, MAX_VISIBLE_CONTRIBUTORS));
  protected hiddenCount = computed(() => Math.max(0, (this.contributors() ?? []).length - MAX_VISIBLE_CONTRIBUTORS));

  protected readonly skeletonWidths = ['7rem', '5.5rem', '6.5rem'];

  protected commitCountLabel(count: number): string {
    return `${count} commit${count > 1 ? 's' : ''}`;
  }

  ngOnInit(): void {
    this.repositories.listContributors(this.repositoryId(), this.ref()).subscribe({
      next: (contributors) => this.contributors.set(contributors),
      error: () => this.contributors.set([]),
    });
  }
}
