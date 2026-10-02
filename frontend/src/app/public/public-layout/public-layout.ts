import { ChangeDetectionStrategy, Component, computed, effect, inject, OnDestroy, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Title } from '@angular/platform-browser';
import { ActivatedRoute, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { Button, GbtInput } from '@masmarino/gabarit';
import { AuthService } from '../../auth/auth.service';
import { PageTitleService } from '../../shell/page-title.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { currentUrl } from '../../shared/current-url';
import { loginLink } from '../../auth/login-link';
import { PUBLIC_CATALOG_MAX_QUERY_LENGTH as MAX_QUERY_LENGTH } from '../public-repositories.service';

const APP_NAME = 'FerrisGit';
const REPOSITORY_URL = 'https://github.com/Masmarino/FerrisGit';

/** The public pages' frame: a header (logo, quick search, docs, sign-in) above a centred column, no sidebar. */
@Component({
  selector: 'fg-public-layout',
  standalone: true,
  imports: [FormsModule, RouterLink, RouterLinkActive, RouterOutlet, Button, GbtInput],
  templateUrl: './public-layout.html',
  styleUrl: './public-layout.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PublicLayout implements OnDestroy {
  private router = inject(Router);
  private url = currentUrl();
  /** Routes that fill the page, like the docs, say so in their data (`{ width: 'full' }`); the rest get a centred column. */
  protected readonly fullWidth = inject(ActivatedRoute).snapshot.data['width'] === 'full';
  private title = inject(Title);
  private pageTitle = inject(PageTitleService);
  private repoContext = inject(RepositoryContextService);
  protected isAuthenticated = inject(AuthService).isAuthenticated;

  protected readonly maxQueryLength = MAX_QUERY_LENGTH;
  protected readonly repositoryUrl = REPOSITORY_URL;
  protected query = signal('');

  protected login = computed(() => loginLink(this.url()));

  constructor() {
    effect(() => {
      const page = this.pageTitle.title();
      const repository = this.repoContext.current()?.path.join('/');
      const parts = [page === APP_NAME ? '' : page, repository ?? '', APP_NAME].filter(Boolean);
      this.title.setTitle(parts.join(' · '));
    });
  }

  ngOnDestroy(): void {
    this.title.setTitle(APP_NAME);
  }

  protected search(): void {
    const q = this.query().trim().slice(0, MAX_QUERY_LENGTH);
    void this.router.navigate(['/explore'], { queryParams: { q: q || null } });
    this.query.set('');
  }
}
