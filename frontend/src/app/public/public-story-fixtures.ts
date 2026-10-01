// Storybook-only fixtures for the public pages.
import { inject, provideAppInitializer, signal } from '@angular/core';
import { HttpErrorResponse } from '@angular/common/http';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router, Routes } from '@angular/router';
import { applicationConfig } from '@storybook/angular-vite';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { GbtToastService } from '@masmarino/gabarit';
import { AuthService } from '../auth/auth.service';
import { PublicConfigService } from './public-config.service';
import { PublicCatalogPage, PublicRepositoriesService, PublicRepositorySummary } from './public-repositories.service';
import { RepositoryContextService } from '../repositories/repository-context.service';
import { PublicRepositoryContextService } from './public-repository-context.service';
import { daysAgo, hoursAgo } from '../shared/layout/page-story-helpers';

export const CATALOG: PublicRepositorySummary[] = [
  { id: 'r1', name: 'ferrisgit', path: ['florian', 'ferrisgit'], owner: 'florian', description: 'Forge Git auto-hébergée écrite en Rust : dépôts, tickets, demandes de fusion, pipelines et wiki dans un seul binaire.', stars: 128, createdAt: daysAgo(320) },
  { id: 'r2', name: 'gabarit', path: ['masmarino', 'design', 'gabarit'], owner: 'florian', description: 'Bibliothèque de composants Angular accessibles, en français.', stars: 64, createdAt: daysAgo(90) },
  { id: 'r3', name: 'ferrisgit-runner', path: ['florian', 'ferrisgit-runner'], owner: 'florian', description: 'Le runner Docker des pipelines FerrisGit.', stars: 17, createdAt: daysAgo(12) },
  { id: 'r4', name: 'notes', path: ['alice', 'notes'], owner: 'alice', description: '', stars: 1, createdAt: hoursAgo(3) },
  {
    id: 'r5',
    name: 'un-depot-au-nom-particulierement-long-pour-verifier-le-retour-a-la-ligne',
    path: ['acme', 'plateforme', 'outils-internes', 'un-depot-au-nom-particulierement-long-pour-verifier-le-retour-a-la-ligne'],
    owner: 'bastien',
    description: 'Une description elle aussi très longue, qui doit être coupée après deux lignes sans jamais faire déborder la carte, même sur un téléphone étroit tenu en portrait.',
    stars: 0,
    createdAt: daysAgo(400),
  },
];

export const catalogPage = (items: PublicRepositorySummary[], total = items.length, page = 1): PublicCatalogPage => ({ items, total, page, perPage: 20 });

const httpError = (status: number) => throwError(() => new HttpErrorResponse({ status, statusText: String(status) }));
export const CATALOG_ERRORS = { rateLimited: () => httpError(429), failed: () => httpError(500), closed: () => httpError(404) };

/** The catalog, its instance switch and a visitor (anonymous unless `signedIn`), with in-memory navigation starting at `url`. */
export function withPublicCatalog(options: { url?: string; search?: () => Observable<PublicCatalogPage>; publicPagesEnabled?: boolean; signedIn?: boolean; routes?: Routes } = {}) {
  return applicationConfig({
    providers: [
      provideRouter(options.routes ?? [{ path: '**', children: [] }]),
      provideLocationMocks(),
      provideAppInitializer(() => inject(Router).navigateByUrl(options.url ?? '/explore')),
      { provide: PublicRepositoriesService, useValue: { search: options.search ?? (() => of(catalogPage(CATALOG))) } },
      { provide: PublicConfigService, useValue: { publicPagesEnabled: () => of(options.publicPagesEnabled ?? true) } },
      { provide: AuthService, useValue: { isAuthenticated: signal(options.signedIn ?? false).asReadonly() } },
      { provide: RepositoryContextService, useClass: PublicRepositoryContextService },
      { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
    ],
  });
}

export const LOADING_CATALOG = () => NEVER;
