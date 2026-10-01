import { Routes } from '@angular/router';
import { authGuard } from './auth/auth.guard';
import { AppShell } from './shell/app-shell';
import { PublicLayout } from './public/public-layout/public-layout';
import { anonymousGuard, publicHomeGuard, publicRepositoryMatcher } from './public/public.guards';
import { providePublicRepositoryData } from './public/public-providers';

const explorePage = () => import('./public/explore-page/explore-page').then((m) => m.ExplorePage);

export const routes: Routes = [
  { path: 'login', loadComponent: () => import('./auth/login-page/login-page').then((m) => m.LoginPage) },
  { path: 'register', loadComponent: () => import('./auth/register-page/register-page').then((m) => m.RegisterPage) },
  { path: 'activate', loadComponent: () => import('./auth/activate-page/activate-page').then((m) => m.ActivatePage) },
  { path: 'reset-password', loadComponent: () => import('./auth/reset-password-page/reset-password-page').then((m) => m.ResetPasswordPage) },
  // Without a session, the same URLs show the public pages: `/` and `/explore` the catalog, `/repositories/<path>` a
  // public repository read-only. Any other known URL falls through to the shell, whose guard sends to the sign-in page.
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
  // A signed-in user can open the catalog too.
  { path: 'explore', component: PublicLayout, children: [{ path: '', loadComponent: explorePage }] },
  {
    path: '',
    component: AppShell,
    canActivate: [authGuard],
    children: [
      { path: '', redirectTo: 'home', pathMatch: 'full' },
      { path: 'home', loadComponent: () => import('./home/home-page/home-page').then((m) => m.HomePage) },
      { path: 'repositories', loadComponent: () => import('./repositories/workspace-page/workspace-page').then((m) => m.WorkspacePage) },
      { path: 'groups', loadComponent: () => import('./groups/groups-redirect/groups-redirect').then((m) => m.GroupsRedirect) },
      { path: 'groups/:id/members', loadComponent: () => import('./groups/group-members/group-members').then((m) => m.GroupMembers) },
      { path: 'repositories/**', loadComponent: () => import('./repositories/repository-path-resolver/repository-path-resolver').then((m) => m.RepositoryPathResolver) },
      { path: 'account', loadComponent: () => import('./account/account-page/account-page').then((m) => m.AccountPage) },
      { path: 'search', loadComponent: () => import('./search/search-results/search-results').then((m) => m.SearchResults) },
      { path: 'runners', loadComponent: () => import('./runners/runners-list/runners-list').then((m) => m.RunnersList) },
      { path: 'admin/settings', loadComponent: () => import('./settings/admin-settings/admin-settings').then((m) => m.AdminSettings) },
      { path: 'admin/users', loadComponent: () => import('./admin/admin-users/admin-users').then((m) => m.AdminUsers) },
      { path: 'admin/users/:id', loadComponent: () => import('./admin/admin-users/admin-user-detail/admin-user-detail').then((m) => m.AdminUserDetail) },
      { path: 'admin/dashboard', loadComponent: () => import('./admin/admin-dashboard/admin-dashboard').then((m) => m.AdminDashboard) },
      { path: 'admin/health', loadComponent: () => import('./admin/admin-health/admin-health').then((m) => m.AdminHealth) },
    ],
  },
  // An unknown URL without a session: the same answer as a private repository.
  {
    path: '**',
    component: PublicLayout,
    canMatch: [anonymousGuard],
    children: [{ path: '', loadComponent: () => import('./public/public-not-found/public-not-found').then((m) => m.PublicNotFound) }],
  },
];
