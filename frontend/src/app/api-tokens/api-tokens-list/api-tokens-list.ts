import { afterNextRender, Component, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { CopyField } from '@masmarino/gabarit/copy-field';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe, formatRelativeTime } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { ApiTokenSummary, TokensService } from '../api-tokens.service';
import { activeLocale } from '../../shared/i18n/translator';

interface RevealedToken {
  id: string;
  name: string;
  token: string;
}

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;

@Component({
  selector: 'fg-api-tokens-list',
  standalone: true,
  imports: [FormsModule, Alert, EmptyState, Badge, Button, GbtInput, Icon, SkeletonList, CopyField, ConfirmDangerModal, ListRow, Card, GbtRelativeTimePipe, GbtDateTimePipe],
  templateUrl: './api-tokens-list.html',
  styleUrl: './api-tokens-list.scss',
})
export class ApiTokensList implements OnInit {
  private tokens = inject(TokensService);
  private toast = inject(GbtToastService);
  private injector = inject(Injector);

  private revealCard = viewChild('revealCard', { read: ElementRef });
  private listCard = viewChild.required('listCard', { read: ElementRef });
  private nameField = viewChild.required('nameField', { read: ElementRef });
  private focusAfterRevoke: number | null = null;


  /** "Créé le 12/08/2026" but "Créé il y a 3 j": the article only goes before an absolute date. */
  protected readonly on = (iso: string) => (/^\d/.test(formatRelativeTime(iso, activeLocale(), undefined, RELATIVE_OPTIONS)) ? 'le ' : '');

  protected list = signal<ApiTokenSummary[]>([]);
  protected listState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected newTokenName = signal('');
  protected creating = signal(false);
  protected revealed = signal<RevealedToken | null>(null);
  protected tokenPendingRevoke = signal<ApiTokenSummary | null>(null);
  protected revoking = signal(false);

  ngOnInit(): void {
    this.refresh();
  }

  refresh(): void {
    this.tokens.list().subscribe({
      next: (list) => {
        this.list.set(list);
        this.listState.set('loaded');
        if (this.focusAfterRevoke !== null) {
          const index = this.focusAfterRevoke;
          this.focusAfterRevoke = null;
          afterNextRender(() => this.focusInList(index), { injector: this.injector });
        }
      },
      error: () => {
        if (this.listState() !== 'loaded') {
          this.listState.set('failed');
        }
        this.toast.show('Impossible de charger les jetons. Réessayez plus tard.', 'error');
      },
    });
  }

  protected retry(): void {
    this.listState.set('loading');
    this.refresh();
  }

  create(): void {
    const name = this.newTokenName().trim();
    if (!name || this.creating()) {
      return;
    }
    this.creating.set(true);
    this.tokens.create(name).subscribe({
      next: (res) => {
        this.creating.set(false);
        this.revealed.set({ id: res.id, name: res.name, token: res.token });
        this.newTokenName.set('');
        this.refresh();
        this.toast.show('Jeton généré.');
        // Emptying the name disables the focused "Générer" button, so focus moves to the token, shown only once. A screen
        // reader reads the card's name, then the token, the warning and the copy button.
        afterNextRender(() => this.focusCard(this.revealCard()), { injector: this.injector });
      },
      error: () => {
        this.creating.set(false);
        this.toast.show('Impossible de générer le jeton.', 'error');
      },
    });
  }

  protected dismissReveal(): void {
    this.revealed.set(null);
    afterNextRender(() => this.nameField().nativeElement.querySelector('input')?.focus(), { injector: this.injector });
  }

  protected confirmRevoke(token: ApiTokenSummary): void {
    this.tokenPendingRevoke.set(token);
  }

  revoke(id: string): void {
    if (this.revoking()) {
      return;
    }
    this.revoking.set(true);
    const index = this.list().findIndex((token) => token.id === id);
    this.tokens.revoke(id).subscribe({
      next: () => {
        // The dialog gives focus back to the row's button, which the refreshed list removes.
        this.focusAfterRevoke = Math.max(index, 0);
        this.revoking.set(false);
        this.tokenPendingRevoke.set(null);
        if (this.revealed()?.id === id) {
          this.revealed.set(null);
        }
        this.toast.show('Jeton révoqué.');
        this.refresh();
      },
      // The confirmation covers the page, so an error has to close it first to be visible.
      error: () => {
        this.revoking.set(false);
        this.tokenPendingRevoke.set(null);
        this.toast.show('Impossible de révoquer ce jeton.', 'error');
      },
    });
  }

  private focusInList(index: number): void {
    const card = this.listCard().nativeElement as HTMLElement;
    const buttons = card.querySelectorAll<HTMLButtonElement>('.api-tokens-list__revoke button');
    const next = buttons[Math.min(index, buttons.length - 1)];
    if (next) {
      next.focus();
    } else {
      this.focusCard(this.listCard());
    }
  }

  /** `gbt-card` has no labelled region to focus, so the heading takes the focus, from script only (`tabindex="-1"`). A screen reader reads it, then the content. */
  private focusCard(host: ElementRef | undefined): void {
    const heading = (host?.nativeElement as HTMLElement | undefined)?.querySelector<HTMLElement>('h2');
    if (heading) {
      heading.tabIndex = -1;
      heading.focus();
    }
  }
}
