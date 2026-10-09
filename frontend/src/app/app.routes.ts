import { Routes } from '@angular/router';
import { authGuard } from './auth/auth.guard';
import { AppShell } from './shell/app-shell';
import { PublicLayout } from './public/public-layout/public-layout';
import { anonymousGuard, publicHomeGuard, publicRepositoryMatcher } from './public/public.guards';
import { providePublicRepositoryData } from './public/public-providers';
import { ADMIN_TRAIL } from './shell/page-trail';
import { DOCS_ROUTES } from './docs/docs.routes';
import { pendingChangesGuard } from './shared/pending-changes';
import { t } from './shared/i18n/translator';

const explorePage = () => import('./public/explore-page/explore-page').then((m) => m.ExplorePage);

// First match wins and a failing canMatch skips the route, so `/`, `/explore`, `/repositories/<path>` and `/docs` each
// have an anonymous route first and a signed-in one after. A URL only the shell knows still reaches it without a
// session, and authGuard sends it to login. Anything unknown ends on the catch-all.
export const routes: Routes = [
  // Sign-in pages, outside both layouts.
  { path: 'login', loadComponent: () => import('./auth/login-page/login-page').then((m) => m.LoginPage) },
  { path: 'register', loadComponent: () => import('./auth/register-page/register-page').then((m) => m.RegisterPage) },
  { path: 'activate', loadComponent: () => import('./auth/activate-page/activate-page').then((m) => m.ActivatePage) },
  { path: 'invitation', loadComponent: () => import('./auth/invitation-page/invitation-page').then((m) => m.InvitationPage) },
  { path: 'reset-password', loadComponent: () => import('./auth/reset-password-page/reset-password-page').then((m) => m.ResetPasswordPage) },

  // Anonymous visitors get the catalog at `/` and `/explore`, and public repositories read-only.
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
  // Docs without a session sit in the public layout, whatever the public pages switch says. With one they sit in the shell.
  { path: 'docs', component: PublicLayout, canMatch: [anonymousGuard], data: { width: 'full' }, children: DOCS_ROUTES },

  // Signed-in users can browse the catalog too, in the same layout.
  { path: 'explore', component: PublicLayout, children: [{ path: '', loadComponent: explorePage }] },

  // Everything else needs a session and lives in the shell.
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

      // Bare `/repositories` is the workspace; anything deeper resolves to a group or a repository.
      { path: 'repositories', loadComponent: () => import('./repositories/workspace-page/workspace-page').then((m) => m.WorkspacePage) },
      // A page with unsaved work (the pipeline editor) asks before any navigation, including to another page of the
      // same repository.
      { path: 'repositories/**', canDeactivate: [pendingChangesGuard], loadComponent: () => import('./repositories/repository-path-resolver/repository-path-resolver').then((m) => m.RepositoryPathResolver) },
      { path: 'groups', loadComponent: () => import('./groups/groups-redirect/groups-redirect').then((m) => m.GroupsRedirect) },
      { path: 'groups/:id/members', loadComponent: () => import('./groups/group-members/group-members').then((m) => m.GroupMembers) },
      { path: 'runners', loadComponent: () => import('./runners/runners-list/runners-list').then((m) => m.RunnersList) },

      { path: 'admin/settings', data: { trail: ADMIN_TRAIL }, loadComponent: () => import('./settings/admin-settings/admin-settings').then((m) => m.AdminSettings) },
      { path: 'admin/users', data: { trail: ADMIN_TRAIL }, loadComponent: () => import('./admin/admin-users/admin-users').then((m) => m.AdminUsers) },
      { path: 'admin/users/:id', data: { trail: [...ADMIN_TRAIL, { get label() { return t('nav.users'); }, link: ['/admin', 'users'] }] }, loadComponent: () => import('./admin/admin-users/admin-user-detail/admin-user-detail').then((m) => m.AdminUserDetail) },
      { path: 'admin/dashboard', data: { trail: ADMIN_TRAIL }, loadComponent: () => import('./admin/admin-dashboard/admin-dashboard').then((m) => m.AdminDashboard) },
      { path: 'admin/health', data: { trail: ADMIN_TRAIL }, loadComponent: () => import('./admin/admin-health/admin-health').then((m) => m.AdminHealth) },
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
