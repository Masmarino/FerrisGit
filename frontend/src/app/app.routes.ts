import { Routes } from '@angular/router';
import { authGuard } from './auth/auth.guard';
import { AppShell } from './shell/app-shell';
import { PublicLayout } from './public/public-layout/public-layout';
import { anonymousGuard, publicHomeGuard, publicRepositoryMatcher } from './public/public.guards';
import { providePublicRepositoryData } from './public/public-providers';
import { DOCS_ROUTES } from './docs/docs.routes';

const explorePage = () => import('./public/explore-page/explore-page').then((m) => m.ExplorePage);

// The router takes the first route that matches the whole URL, and a route whose `canMatch` says no is skipped. Two
// routes therefore exist for several URLs (`/`, `/explore`, `/repositories/<path>`, `/docs`): the anonymous one comes
// first and the signed-in one after it. Without a session, a URL that only the shell knows still reaches the shell,
// where `authGuard` sends it to the sign-in page. A URL nobody knows ends on the catch-all at the bottom.
export const routes: Routes = [
  // Sign-in family: open to everyone, outside both layouts.
  { path: 'login', loadComponent: () => import('./auth/login-page/login-page').then((m) => m.LoginPage) },
  { path: 'register', loadComponent: () => import('./auth/register-page/register-page').then((m) => m.RegisterPage) },
  { path: 'activate', loadComponent: () => import('./auth/activate-page/activate-page').then((m) => m.ActivatePage) },
  { path: 'reset-password', loadComponent: () => import('./auth/reset-password-page/reset-password-page').then((m) => m.ResetPasswordPage) },

  // Without a session: the catalog at `/` and `/explore`, a public repository read-only at `/repositories/<path>`.
  {
    path: '',
    component: PublicLayout,
    canMatch: [anonymousGuard],
    providers: providePublicRepositoryData(),
    children: [
      { path: '', pathMatch: 'full', canActivate: [publicHomeGuard], loadComponent: explorePage },
      { path: 'explore', loadComponent: explorePage },
      { matcher: publicRepositoryMatcher, loadComponent: () => import('./public/public-repository-page/public-repository-page').then((m) => m.PublicRepositoryPage) },
    ],
  },
  // The documentation without a session: in the public layout whatever the public pages switch says. With a session it
  // is in the shell (see below).
  { path: 'docs', component: PublicLayout, canMatch: [anonymousGuard], data: { width: 'full' }, children: DOCS_ROUTES },

  // A signed-in user can open the catalog too, in the same public layout.
  { path: 'explore', component: PublicLayout, children: [{ path: '', loadComponent: explorePage }] },

  // With a session: everything else lives in the shell.
  {
    path: '',
    component: AppShell,
    canActivate: [authGuard],
    children: [
      { path: '', redirectTo: 'home', pathMatch: 'full' },
      { path: 'home', loadComponent: () => import('./home/home-page/home-page').then((m) => m.HomePage) },
      { path: 'search', loadComponent: () => import('./search/search-results/search-results').then((m) => m.SearchResults) },
      { path: 'docs', children: DOCS_ROUTES },
      { path: 'account', loadComponent: () => import('./account/account-page/account-page').then((m) => m.AccountPage) },

      // The workspace is `/repositories` alone; anything below it is resolved to a group or a repository.
      { path: 'repositories', loadComponent: () => import('./repositories/workspace-page/workspace-page').then((m) => m.WorkspacePage) },
      { path: 'repositories/**', loadComponent: () => import('./repositories/repository-path-resolver/repository-path-resolver').then((m) => m.RepositoryPathResolver) },
      { path: 'groups', loadComponent: () => import('./groups/groups-redirect/groups-redirect').then((m) => m.GroupsRedirect) },
      { path: 'groups/:id/members', loadComponent: () => import('./groups/group-members/group-members').then((m) => m.GroupMembers) },
      { path: 'runners', loadComponent: () => import('./runners/runners-list/runners-list').then((m) => m.RunnersList) },

      { path: 'admin/settings', loadComponent: () => import('./settings/admin-settings/admin-settings').then((m) => m.AdminSettings) },
      { path: 'admin/users', loadComponent: () => import('./admin/admin-users/admin-users').then((m) => m.AdminUsers) },
      { path: 'admin/users/:id', loadComponent: () => import('./admin/admin-users/admin-user-detail/admin-user-detail').then((m) => m.AdminUserDetail) },
      { path: 'admin/dashboard', loadComponent: () => import('./admin/admin-dashboard/admin-dashboard').then((m) => m.AdminDashboard) },
      { path: 'admin/health', loadComponent: () => import('./admin/admin-health/admin-health').then((m) => m.AdminHealth) },
    ],
  },

  // Without a session, an unknown URL gets the same answer as a private repository.
  {
    path: '**',
    component: PublicLayout,
    canMatch: [anonymousGuard],
    children: [{ path: '', loadComponent: () => import('./public/public-not-found/public-not-found').then((m) => m.PublicNotFound) }],
  },
];
