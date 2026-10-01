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

  /** A stable empty path rather than a fresh `[]` literal bound to an input. */
  protected readonly rootPath: string[] = [];

  private pageTitle = inject(PageTitleService);

  ngOnInit(): void {
    // The overview has no breadcrumb segment of its own. Reset it so a subpage title does not linger after navigating back here.
    this.pageTitle.set('');
  }
}
