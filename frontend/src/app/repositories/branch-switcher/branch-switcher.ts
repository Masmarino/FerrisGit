import { Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { BranchInfo, MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { ReleasesService } from '../../releases/releases.service';
import { repositoryLink } from '../repository-links';

@Component({
  selector: 'fg-branch-switcher',
  standalone: true,
  imports: [FormsModule, Select],
  templateUrl: './branch-switcher.html',
  styleUrl: './branch-switcher.scss',
})
export class BranchSwitcher implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();
  currentRef = input.required<string>();

  private mergeRequests = inject(MergeRequestsService);
  private releases = inject(ReleasesService);
  private router = inject(Router);

  private branches = signal<BranchInfo[]>([]);
  private tagNames = signal<string[]>([]);

  protected refOptions = computed<SelectOption<string>[]>(() => [
    ...this.branches().map((b) => ({ value: b.name, label: b.name, icon: 'git-branch' })),
    ...this.tagNames().map((name) => ({ value: name, label: name, icon: 'tag' })),
  ]);

  // On the bare repository view the ref is 'HEAD', which matches no option and would leave the select
  // on its placeholder, so show the default branch instead.
  protected selectedRef = computed(() => {
    if (this.currentRef() !== 'HEAD') return this.currentRef();
    return this.branches().find((b) => b.isDefault)?.name ?? this.currentRef();
  });

  ngOnInit(): void {
    this.mergeRequests.listBranches(this.repositoryId()).subscribe((branches) => this.branches.set(branches));
    this.releases.listTags(this.repositoryId()).subscribe((tags) => this.tagNames.set(tags.map((t) => t.name)));
  }

  navigateToRef(ref: string) {
    return this.router.navigate(repositoryLink(this.path(), 'tree', ref));
  }
}
