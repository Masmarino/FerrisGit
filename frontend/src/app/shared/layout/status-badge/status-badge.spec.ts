import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { Badge } from '@masmarino/gabarit/badge';
import { Icon, IconRegistry } from '@masmarino/gabarit/icon';
import { provideFerrisgitIcons } from '../../register-icons';
import { StatusBadge, statusPresentation, StatusKind } from './status-badge';

@Component({
  imports: [StatusBadge],
  template: `<fg-status-badge [kind]="kind()" [status]="status()" />`,
})
class StatusBadgeHost {
  kind = signal<StatusKind>('issue');
  status = signal('todo');
}

function render(kind: StatusKind, status: string) {
  const fixture = TestBed.createComponent(StatusBadgeHost);
  fixture.componentInstance.kind.set(kind);
  fixture.componentInstance.status.set(status);
  fixture.detectChanges();
  const badge = fixture.debugElement.query(By.directive(Badge));
  const pill = fixture.nativeElement.querySelector('.gbt-badge') as HTMLElement;
  const icon = badge.query(By.directive(Icon))?.componentInstance as Icon | undefined;
  return { fixture, badge: badge.componentInstance as Badge, pill, icon };
}

describe('StatusBadge', () => {
  const cases: [StatusKind, string, string, string, string][] = [
    ['issue', 'todo', 'À faire', 'neutral', 'circle-dot'],
    ['issue', 'in_progress', 'En cours', 'info', 'circle-half'],
    ['issue', 'in_review', 'En revue', 'warning', 'circle-three-quarters'],
    ['issue', 'done', 'Terminé', 'success', 'circle-check'],
    ['merge-request', 'open', 'Ouverte', 'info', 'git-pull-request'],
    ['merge-request', 'merged', 'Fusionnée', 'success', 'git-merge'],
    ['merge-request', 'closed', 'Fermée', 'neutral', 'git-pull-request-closed'],
    ['pipeline', 'pending', 'En attente', 'neutral', 'clock'],
    ['pipeline', 'running', 'En cours', 'info', 'circle-play'],
    ['pipeline', 'success', 'Réussie', 'success', 'circle-check'],
    ['pipeline', 'failed', 'Échouée', 'error', 'circle-x'],
    ['pipeline', 'canceled', 'Annulée', 'neutral', 'circle-slash'],
    ['release', 'draft', 'Brouillon', 'neutral', 'pencil'],
    ['release', 'prerelease', 'Pré-version', 'warning', 'flask-conical'],
    ['release', 'published', 'Publiée', 'success', 'tag'],
  ];

  for (const [kind, status, label, variant, iconName] of cases) {
    it(`renders ${kind} "${status}" as "${label}" (${variant}) with the ${iconName} icon`, () => {
      const { badge, pill, icon } = render(kind, status);

      expect(pill.textContent?.trim()).toBe(label);
      expect(badge.variant()).toBe(variant);
      expect(pill.getAttribute('data-variant')).toBe(variant);
      expect(icon?.name()).toBe(iconName);
    });
  }

  it('renders an unknown status as its raw text, neutral, with the kind’s generic icon', () => {
    const issue = render('issue', 'blocked');
    expect(issue.pill.textContent?.trim()).toBe('blocked');
    expect(issue.pill.getAttribute('data-variant')).toBe('neutral');
    expect(issue.icon?.name()).toBe('circle-dot');

    const mergeRequest = render('merge-request', 'draft');
    expect(mergeRequest.pill.textContent?.trim()).toBe('draft');
    expect(mergeRequest.pill.getAttribute('data-variant')).toBe('neutral');
    expect(mergeRequest.icon?.name()).toBe('git-pull-request');

    const pipeline = render('pipeline', 'skipped');
    expect(pipeline.pill.textContent?.trim()).toBe('skipped');
    expect(pipeline.pill.getAttribute('data-variant')).toBe('neutral');
    expect(pipeline.icon?.name()).toBe('circle-dot');

    const release = render('release', 'archived');
    expect(release.pill.textContent?.trim()).toBe('archived');
    expect(release.pill.getAttribute('data-variant')).toBe('neutral');
    expect(release.icon?.name()).toBe('tag');
  });

  it('follows status changes', () => {
    const { fixture, pill } = render('issue', 'todo');
    fixture.componentInstance.status.set('done');
    fixture.detectChanges();

    expect(pill.textContent?.trim()).toBe('Terminé');
    expect(pill.getAttribute('data-variant')).toBe('success');
  });

  it('keeps the icon decorative: the label is the accessible text', () => {
    const { fixture } = render('merge-request', 'merged');
    const iconHost = fixture.nativeElement.querySelector('gbt-icon') as HTMLElement;
    expect(iconHost.getAttribute('aria-hidden')).toBe('true');
  });

  it('registers every icon it uses (an unregistered name renders an empty box)', () => {
    TestBed.configureTestingModule({ providers: [provideFerrisgitIcons()] });
    const registry = TestBed.inject(IconRegistry);
    for (const [kind, status] of cases) {
      const icon = statusPresentation(kind, status).icon;
      expect(registry.get(icon), `${kind} ${status}: ${icon}`).not.toBeNull();
    }
  });

  describe('statusPresentation', () => {
    it('is the single source of the label, variant and icon (for row leading icons)', () => {
      expect(statusPresentation('issue', 'in_review')).toEqual({ label: 'En revue', variant: 'warning', icon: 'circle-three-quarters' });
      expect(statusPresentation('merge-request', 'merged')).toEqual({ label: 'Fusionnée', variant: 'success', icon: 'git-merge' });
      expect(statusPresentation('pipeline', 'failed')).toEqual({ label: 'Échouée', variant: 'error', icon: 'circle-x' });
      expect(statusPresentation('release', 'prerelease')).toEqual({ label: 'Pré-version', variant: 'warning', icon: 'flask-conical' });
    });

    it('does not treat inherited object keys as statuses', () => {
      expect(statusPresentation('pipeline', 'toString')).toEqual({ label: 'toString', variant: 'neutral', icon: 'circle-dot' });
    });

    it('returns the same object for the same status (safe to bind without re-creating)', () => {
      expect(statusPresentation('issue', 'todo')).toBe(statusPresentation('issue', 'todo'));
    });
  });
});
