import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { Badge } from '@masmarino/gabarit/badge';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { ListCard, ListCardState } from '@masmarino/gabarit/list-card';
import { ListRow } from '@masmarino/gabarit/list-row';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { CommitInfo, RepositoriesService } from '../repositories.service';
import { commitTitle, shortSha } from '../commit-format';
import { PageTitleService } from '../../shell/page-title.service';

/** The latest commits of a ref, newest first. */
@Component({
  selector: 'fg-repository-commit-list',
  standalone: true,
  imports: [Badge, ListCard, ListRow, PageHeader, PageLayout, UserChip, GbtDateTimePipe, GbtRelativeTimePipe],
  templateUrl: './repository-commit-list.html',
  styleUrl: './repository-commit-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RepositoryCommitList implements OnInit {
  repositoryId = input.required<string>();
  ref = input('HEAD');

  private repositories = inject(RepositoriesService);
  private pageTitle = inject(PageTitleService);

  protected commits = signal<CommitInfo[]>([]);
  protected loadState = signal<'loading' | 'failed' | 'loaded'>('loading');
  protected listState = computed<ListCardState>(() => {
    const state = this.loadState();
    if (state !== 'loaded') return state;
    return this.commits().length === 0 ? 'empty' : 'ready';
  });
  protected refLabel = computed(() => (this.ref() === 'HEAD' ? 'la branche par défaut' : this.ref()));
  protected commitTitle = commitTitle;

  ngOnInit(): void {
    this.pageTitle.set('Commits');
    this.load();
  }

  protected load(): void {
    this.loadState.set('loading');
    this.repositories.commitsById(this.repositoryId(), this.ref() === 'HEAD' ? undefined : this.ref()).subscribe({
      next: (commits) => {
        this.commits.set(commits);
        this.loadState.set('loaded');
      },
      error: () => this.loadState.set('failed'),
    });
  }

  protected shortSha = shortSha;
}
