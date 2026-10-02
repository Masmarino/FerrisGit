import { Component, inject, input, OnInit } from '@angular/core';
import { RepositoryTreeView } from '../repository-tree-view/repository-tree-view';
import { PageTitleService } from '../../shell/page-title.service';

@Component({
  selector: 'fg-repository-detail',
  standalone: true,
  imports: [RepositoryTreeView],
  templateUrl: './repository-detail.html',
})
export class RepositoryDetail implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  /** Stable reference, unlike a `[]` literal bound in the template. */
  protected readonly rootPath: string[] = [];

  private pageTitle = inject(PageTitleService);

  ngOnInit(): void {
    // The overview has no breadcrumb segment, so clear whatever a subpage left behind.
    this.pageTitle.set('');
  }
}
