import { ChangeDetectionStrategy, Component, inject, OnInit } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { PageTitleService } from '../../shell/page-title.service';
import { loginLink } from '../../auth/login-link';

/** One answer for an unknown page, an unknown repository and a private one, so a visitor can't tell which. */
@Component({
  selector: 'fg-public-not-found',
  standalone: true,
  imports: [RouterLink, Button, Card, EmptyState],
  templateUrl: './public-not-found.html',
  styleUrl: './public-not-found.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PublicNotFound implements OnInit {
  private pageTitle = inject(PageTitleService);
  protected readonly login = loginLink(inject(Router).url);

  ngOnInit(): void {
    this.pageTitle.set('Introuvable');
  }
}
