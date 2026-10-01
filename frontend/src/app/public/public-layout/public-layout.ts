import { ChangeDetectionStrategy, Component, computed, effect, inject, OnDestroy, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { Title } from '@angular/platform-browser';
import { NavigationEnd, Router, RouterLink, RouterOutlet } from '@angular/router';
import { filter, map } from 'rxjs';
import { Button, GbtInput } from '@masmarino/gabarit';
import { AuthService } from '../../auth/auth.service';
import { PageTitleService } from '../../shell/page-title.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { loginLink } from '../login-link';

const APP_NAME = 'FerrisGit';
const MAX_QUERY_LENGTH = 100;

/** The frame of the public pages: a header (logo, quick search, sign-in) above a centred content column. No sidebar. */
@Component({
  selector: 'fg-public-layout',
  standalone: true,
  imports: [FormsModule, RouterLink, RouterOutlet, Button, GbtInput],
  templateUrl: './public-layout.html',
  styleUrl: './public-layout.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PublicLayout implements OnDestroy {
  private router = inject(Router);
  private title = inject(Title);
  private pageTitle = inject(PageTitleService);
  private repoContext = inject(RepositoryContextService);
  protected isAuthenticated = inject(AuthService).isAuthenticated;

  protected readonly maxQueryLength = MAX_QUERY_LENGTH;
  protected query = signal('');

  private currentUrl = toSignal(
    this.router.events.pipe(
      filter((event): event is NavigationEnd => event instanceof NavigationEnd),
      map((event) => event.urlAfterRedirects),
    ),
    { initialValue: this.router.url },
  );
  protected login = computed(() => loginLink(this.currentUrl()));

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
