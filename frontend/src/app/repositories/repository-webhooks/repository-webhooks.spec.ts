import { LOCALE_ID } from '@angular/core';
import { of, Subject, throwError } from 'rxjs';
import { RepositoryWebhooks } from './repository-webhooks';
import { RepositorySettingsService, WebhookDelivery, WebhookSummary } from '../repository-settings.service';
import { createSettingsSection } from '../settings-section-testing';

describe('RepositoryWebhooks', () => {
  function setup() {
    const repositorySettingsStub = {
      listWebhooks: vi.fn(() => of<WebhookSummary[]>([])),
      createWebhook: vi.fn(() => of<WebhookSummary | undefined>(undefined)),
      deleteWebhook: vi.fn(() => of<void>(undefined)),
      listWebhookDeliveries: vi.fn(() => of<WebhookDelivery[]>([])),
    };
    const providers = [
      { provide: RepositorySettingsService, useValue: repositorySettingsStub },
      { provide: LOCALE_ID, useValue: 'fr' },
    ];
    return { ...createSettingsSection(RepositoryWebhooks, providers), repositorySettingsStub };
  }

  it('lists configured webhooks', () => {
    const { fixture, repositorySettingsStub } = setup();
    repositorySettingsStub.listWebhooks.mockReturnValue(
      of([{ id: 'w1', url: 'https://example.com', events: ['issue_closed'], active: true, createdAt: '2026-01-01T00:00:00Z' }]),
    );
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('https://example.com');
  });

  it('surfaces an error when loading webhooks fails', () => {
    const { fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.listWebhooks.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });

  it('does not add a webhook when no event is selected', () => {
    const { component, repositorySettingsStub } = setup();

    component['newWebhookUrl'].set('https://example.com');
    component['newWebhookSecret'].set('shh');
    component.addWebhook();

    expect(repositorySettingsStub.createWebhook).not.toHaveBeenCalled();
  });

  it('adds a webhook with the selected events and clears the form', () => {
    const { component, repositorySettingsStub } = setup();
    repositorySettingsStub.createWebhook.mockReturnValue(
      of({ id: 'w1', url: 'https://example.com', events: ['issue_closed'], active: true, createdAt: '2026-01-01T00:00:00Z' }),
    );

    component['newWebhookUrl'].set('https://example.com');
    component['newWebhookSecret'].set('shh');
    component['newWebhookEvents'].set(['issue_closed']);
    component.addWebhook();

    expect(repositorySettingsStub.createWebhook).toHaveBeenCalledWith('repo-1', 'https://example.com', 'shh', ['issue_closed']);
    expect(component['newWebhookUrl']()).toBe('');
    expect(component['newWebhookSecret']()).toBe('');
  });

  it('loads and displays delivery history when toggled', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    // The drawer heading looks the URL up in the loaded list, so the list has to be there.
    repositorySettingsStub.listWebhooks.mockReturnValue(
      of([{ id: 'w1', url: 'https://example.com', events: ['issue_closed'], active: true, createdAt: '2026-01-01T00:00:00Z' }]),
    );
    repositorySettingsStub.listWebhookDeliveries.mockReturnValue(
      of([{ id: 'd1', eventKind: 'issue_closed', httpStatus: 200, success: true, errorMessage: null, createdAt: '2026-01-01T00:00:00Z' }]),
    );
    fixture.detectChanges();

    component.toggleDeliveries('w1');
    fixture.detectChanges();

    expect(repositorySettingsStub.listWebhookDeliveries).toHaveBeenCalledWith('repo-1', 'w1');
    expect(fixture.nativeElement.textContent).toContain('Succès');
  });

  it('keeps the checkbox visually checked after a change-detection pass, and unchecks it once the webhook is added', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    repositorySettingsStub.createWebhook.mockReturnValue(
      of({ id: 'w1', url: 'https://example.com', events: ['issue_closed'], active: true, createdAt: '2026-01-01T00:00:00Z' }),
    );
    fixture.detectChanges();

    const checkbox: HTMLInputElement = fixture.nativeElement.querySelectorAll('gbt-checkbox-group input[type="checkbox"]')[0];
    checkbox.click();
    fixture.detectChanges();

    expect(component['newWebhookEvents']().length).toBe(1);
    expect(checkbox.checked).toBe(true);

    component['newWebhookUrl'].set('https://example.com');
    component['newWebhookSecret'].set('shh');
    component.addWebhook();
    fixture.detectChanges();

    expect(component['newWebhookEvents']().length).toBe(0);
    expect(checkbox.checked).toBe(false);
  });

  it('surfaces an error and keeps the form filled in when adding a webhook fails', () => {
    const { component, fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.createWebhook.mockReturnValue(throwError(() => ({ status: 400 })));

    component['newWebhookUrl'].set('https://example.com');
    component['newWebhookSecret'].set('shh');
    component['newWebhookEvents'].set(['issue_closed']);
    component.addWebhook();
    fixture.detectChanges();

    expect(component['newWebhookUrl']()).toBe('https://example.com');
    expect(toastStub.show).toHaveBeenCalledWith("Impossible d'ajouter ce webhook (URL invalide ou pointant vers une adresse interdite).", 'error');
  });

  it('surfaces an error when loading delivery history fails', () => {
    const { component, fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.listWebhookDeliveries.mockReturnValue(throwError(() => ({ status: 500 })));

    component.toggleDeliveries('w1');
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith("Impossible de charger l'historique des livraisons.", 'error');
    // The toast announces it; the drawer shows it too, but silently.
    const failed = (fixture.nativeElement as HTMLElement).querySelector('gbt-drawer gbt-alert .gbt-alert');
    expect(failed?.textContent?.trim()).toBe("L'historique n'a pas pu être chargé.");
    expect(failed?.getAttribute('data-variant')).toBe('error');
    expect(failed?.getAttribute('role')).toBeNull();
    expect(failed?.getAttribute('aria-live')).toBeNull();
  });

  it('re-fetches only this section after adding a webhook, not on construction alone', () => {
    const { component, repositorySettingsStub } = setup();
    repositorySettingsStub.createWebhook.mockReturnValue(
      of({ id: 'w1', url: 'https://example.com', events: ['issue_closed'], active: true, createdAt: '2026-01-01T00:00:00Z' }),
    );

    component['newWebhookUrl'].set('https://example.com');
    component['newWebhookSecret'].set('shh');
    component['newWebhookEvents'].set(['issue_closed']);
    component.addWebhook();

    // ngOnInit never ran here, so this call is the refresh after addWebhook().
    expect(repositorySettingsStub.listWebhooks).toHaveBeenCalledTimes(1);
  });

  describe('layout', () => {
    const WEBHOOKS: WebhookSummary[] = [
      { id: 'w1', url: 'https://ci.exemple.fr/hooks', events: ['pipeline_failed', 'merge_request_merged'], active: true, createdAt: '2026-01-10T09:00:00Z' },
      {
        id: 'w2',
        url: 'https://chat.exemple.fr/notifications',
        events: ['merge_request_approved', 'merge_request_commented', 'issue_assigned', 'issue_closed', 'collaborator_added'],
        active: false,
        createdAt: '2026-02-18T14:30:00Z',
      },
    ];
    const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
    const button = (root: Element, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label || b.getAttribute('aria-label') === label);

    function loaded(webhooks: WebhookSummary[] = WEBHOOKS) {
      const ctx = setup();
      ctx.repositorySettingsStub.listWebhooks.mockReturnValue(of(webhooks));
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }

    it('has a creation card and a list card with the number of webhooks', () => {
      const { el } = loaded();

      expect(Array.from(el.querySelectorAll('gbt-card h2')).map(text)).toEqual(['Ajouter un webhook', 'Webhooks 2']);
      expect(el.querySelectorAll('gbt-card .gbt-card__description')).toHaveLength(2);
    });

    it('offers every event as a labelled checkbox, grouped by subject', () => {
      const { el } = loaded();

      const fieldset = el.querySelector('gbt-checkbox-group.repository-webhooks__webhook-events fieldset')!;
      expect(text(fieldset.querySelector('legend'))).toBe('Événements');
      const groups = Array.from(fieldset.querySelectorAll('[role="group"]'));
      expect(groups.map((group) => text(el.querySelector(`#${group.getAttribute('aria-labelledby')}`)))).toEqual(['Demandes de fusion', 'Tickets', 'Pipelines', 'Collaborateurs']);
      expect(groups.map((group) => group.querySelectorAll('input[type="checkbox"]').length)).toEqual([5, 3, 1, 3]);
      const labels = Array.from(fieldset.querySelectorAll('label')).map(text);
      expect(labels).toContain('Fusionnée');
      expect(labels).toContain('Échoué');
      expect(labels.every((label) => !!label && !label.includes('_'))).toBe(true);
    });

    it('keeps the selected events as an array, in the order of the groups, whatever the order they were ticked in', () => {
      const { component, fixture, el } = loaded();
      const box = (label: string) =>
        Array.from(el.querySelectorAll<HTMLInputElement>('gbt-checkbox-group input[type="checkbox"]')).find((input) => text(input.closest('label')) === label)!;

      box('Échoué').click();
      box('Fusionnée').click();
      fixture.detectChanges();
      expect(component['newWebhookEvents']()).toEqual(['merge_request_merged', 'pipeline_failed']);

      box('Fusionnée').click();
      fixture.detectChanges();
      expect(component['newWebhookEvents']()).toEqual(['pipeline_failed']);
    });

    it('sums up the selection next to the submit button', () => {
      const { component, fixture, el } = loaded();
      const summary = () => text(el.querySelector('.repository-webhooks__summary'));

      expect(summary()).toBe('Aucun événement sélectionné');
      component['newWebhookEvents'].set(['issue_closed']);
      fixture.detectChanges();
      expect(summary()).toBe('1 événement sélectionné');
      component['newWebhookEvents'].set(['issue_closed', 'pipeline_failed']);
      fixture.detectChanges();
      expect(summary()).toBe('2 événements sélectionnés');
    });

    it('adds the webhook when the form is submitted (Enter in a field)', () => {
      const { component, el, repositorySettingsStub } = loaded();
      component['newWebhookUrl'].set('https://example.com');
      component['newWebhookSecret'].set('shh');
      component['newWebhookEvents'].set(['issue_closed']);

      el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

      expect(repositorySettingsStub.createWebhook).toHaveBeenCalledWith('repo-1', 'https://example.com', 'shh', ['issue_closed']);
    });

    it('lists each webhook as a row: its URL, its state in words, its events in French (the first three, then a count)', () => {
      const { el } = loaded();

      const rows = Array.from(el.querySelectorAll('ul > li > gbt-list-row'));
      expect(rows.map((row) => text(row.querySelector('.repository-webhooks__url')))).toEqual(['https://ci.exemple.fr/hooks', 'https://chat.exemple.fr/notifications']);
      expect(rows.map((row) => text(row.querySelector('gbt-badge')))).toEqual(['Actif', 'Inactif']);
      expect(Array.from(rows[0].querySelectorAll('.repository-webhooks__event')).map(text)).toEqual(['Pipeline échoué', 'Demande de fusion fusionnée']);
      const second = Array.from(rows[1].querySelectorAll('.repository-webhooks__event')).map(text);
      expect(second).toEqual(['Demande de fusion approuvée', 'Demande de fusion commentée', 'Ticket assigné', '+2']);
      expect(rows[1].querySelector('.repository-webhooks__event--more')?.getAttribute('title')).toBe('Ticket fermé, Collaborateur ajouté');
    });

    it('shows skeleton rows while the webhooks load, and explains the empty list', () => {
      const { fixture, repositorySettingsStub } = setup();
      const pending = new Subject<WebhookSummary[]>();
      repositorySettingsStub.listWebhooks.mockReturnValue(pending);
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(2);
      // One polite status, outside any aria-busy region (those can hold back the announcement).
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des webhooks…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();

      pending.next([]);
      fixture.detectChanges();
      expect(el.querySelector('gbt-skeleton-list')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain('Aucun webhook configuré');
    });

    it('shows a failed state with a retry button when the list cannot be loaded', () => {
      const { fixture, repositorySettingsStub } = setup();
      repositorySettingsStub.listWebhooks.mockReturnValue(throwError(() => ({ status: 500 })));
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les webhooks n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The toast announces the failure, so the inline block stays silent.
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"]')).toHaveLength(0);
      expect(failed?.closest('[aria-live]')).toBeNull();

      repositorySettingsStub.listWebhooks.mockReturnValue(of(WEBHOOKS));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it("opens a row's delivery history from its button; deliveries read their result in words", () => {
      const { fixture, el, repositorySettingsStub } = loaded();
      const pending = new Subject<WebhookDelivery[]>();
      repositorySettingsStub.listWebhookDeliveries.mockReturnValue(pending);

      button(el.querySelectorAll('gbt-list-row')[0], 'Historique des livraisons de https://ci.exemple.fr/hooks')!.click();
      fixture.detectChanges();
      expect(repositorySettingsStub.listWebhookDeliveries).toHaveBeenCalledWith('repo-1', 'w1');
      const drawer = () => el.querySelector('gbt-drawer')!;
      expect(drawer().querySelector('[aria-busy="true"]')).not.toBeNull();

      pending.next([
        { id: 'd1', eventKind: 'pipeline_failed', httpStatus: 200, success: true, errorMessage: null, createdAt: '2026-09-20T08:12:00Z' },
        { id: 'd2', eventKind: 'merge_request_merged', httpStatus: null, success: false, errorMessage: 'Délai dépassé', createdAt: '2026-09-21T08:12:00Z' },
      ]);
      fixture.detectChanges();
      const deliveries = Array.from(drawer().querySelectorAll('li'));
      expect(deliveries.map((li) => text(li.querySelector('gbt-badge')))).toEqual(['Succès', 'Échec']);
      expect(text(deliveries[0])).toContain('Pipeline échoué');
      expect(text(deliveries[0])).toContain('HTTP 200');
      expect(text(deliveries[1])).toContain('Délai dépassé');
    });

    const dialog = (el: HTMLElement) => el.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');

    it('confirms before deleting, without making the user retype the URL: confirming deletes that webhook and drops its row', () => {
      const { fixture, el, repositorySettingsStub, toastStub } = loaded();

      button(el.querySelectorAll('gbt-list-row')[1], 'Supprimer le webhook https://chat.exemple.fr/notifications')!.click();
      fixture.detectChanges();

      const modal = dialog(el)!;
      expect(modal.getAttribute('aria-label')).toBe('Supprimer le webhook');
      expect(text(modal)).toContain('https://chat.exemple.fr/notifications');
      expect(modal.querySelector('input')).toBeNull();
      expect(button(modal, 'Supprimer')!.disabled).toBe(false);
      expect(repositorySettingsStub.deleteWebhook).not.toHaveBeenCalled();

      repositorySettingsStub.listWebhooks.mockReturnValue(of([WEBHOOKS[0]]));
      button(modal, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.deleteWebhook).toHaveBeenCalledExactlyOnceWith('repo-1', 'w2');
      expect(dialog(el)).toBeNull();
      expect(Array.from(el.querySelectorAll('.repository-webhooks__url')).map(text)).toEqual(['https://ci.exemple.fr/hooks']);
      expect(toastStub.show).toHaveBeenCalledWith('Webhook supprimé.');
    });

    it('keeps the confirmation open and busy while the deletion runs: a second click sends nothing', () => {
      const { fixture, el, repositorySettingsStub } = loaded();
      const pending = new Subject<void>();
      repositorySettingsStub.deleteWebhook.mockReturnValue(pending);

      button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer le webhook https://ci.exemple.fr/hooks')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(dialog(el)).not.toBeNull();
      expect(dialog(el)!.querySelector('.gbt-button--danger')!.getAttribute('aria-busy')).toBe('true');
      dialog(el)!.querySelector<HTMLButtonElement>('.gbt-button--danger')!.click();
      button(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(repositorySettingsStub.deleteWebhook).toHaveBeenCalledTimes(1);
      expect(dialog(el)).not.toBeNull();

      pending.next();
      pending.complete();
      fixture.detectChanges();
      expect(dialog(el)).toBeNull();
    });

    it('says it is deleting, and ignores Escape, the backdrop and the close button until the request is done', () => {
      const { fixture, el, repositorySettingsStub } = loaded();
      const pending = new Subject<void>();
      repositorySettingsStub.deleteWebhook.mockReturnValue(pending);

      button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer le webhook https://ci.exemple.fr/hooks')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(text(dialog(el)!.querySelector('[role="status"]'))).toBe('Suppression en cours');
      expect(button(dialog(el)!, 'Annuler')!.disabled).toBe(true);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      el.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      button(dialog(el)!, 'Fermer')!.click();
      fixture.detectChanges();
      expect(dialog(el)).not.toBeNull();
      expect(repositorySettingsStub.deleteWebhook).toHaveBeenCalledTimes(1);
    });

    it('closes from Escape, the backdrop and the close button when idle, and gives the focus back to the row button that opened it', async () => {
      const { fixture, el } = loaded();
      const opener = button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer le webhook https://ci.exemple.fr/hooks')!;

      for (const dismiss of [
        () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })),
        () => el.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click(),
        () => button(dialog(el)!, 'Fermer')!.click(),
        () => button(dialog(el)!, 'Annuler')!.click(),
      ]) {
        opener.focus();
        opener.click();
        fixture.detectChanges();
        await fixture.whenStable();
        expect(dialog(el)).not.toBeNull();
        // The dialog took focus (it isn't still on the opener), so getting it back is a real move.
        expect(document.activeElement).toBe(dialog(el));

        dismiss();
        fixture.detectChanges();
        expect(dialog(el)).toBeNull();
        expect(document.activeElement).toBe(opener);
      }
    });

    it('closes the confirmation and says so when deleting fails, keeping the row', () => {
      const { fixture, el, repositorySettingsStub, toastStub } = loaded();
      repositorySettingsStub.deleteWebhook.mockReturnValue(throwError(() => ({ status: 500 })));

      button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer le webhook https://ci.exemple.fr/hooks')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.deleteWebhook).toHaveBeenCalledWith('repo-1', 'w1');
      expect(dialog(el)).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de supprimer ce webhook.', 'error');
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    it('closes the confirmation without deleting from "Annuler"', () => {
      const { fixture, el, repositorySettingsStub } = loaded();

      button(el.querySelectorAll('gbt-list-row')[0], 'Supprimer le webhook https://ci.exemple.fr/hooks')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog(el)).toBeNull();
      expect(repositorySettingsStub.deleteWebhook).not.toHaveBeenCalled();
    });
  });
});
