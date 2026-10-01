import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { LinkMailFailed } from './link-mail-failed';

@Component({
  imports: [LinkMailFailed],
  template: `<fg-link-mail-failed [username]="username" [emailError]="emailError()" [url]="activationUrl()" [dismissible]="dismissible" (dismissed)="dismissedCount = dismissedCount + 1" />`,
})
class Host {
  username = 'bob';
  emailError = signal<string | undefined>('connection refused');
  activationUrl = signal<string | undefined>('http://localhost:4200/activate#token=abc123');
  dismissible = false;
  dismissedCount = 0;
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('LinkMailFailed', () => {
  let clipboardDescriptor: PropertyDescriptor | undefined;

  beforeEach(() => {
    clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
  });

  afterEach(() => {
    if (clipboardDescriptor) {
      Object.defineProperty(navigator, 'clipboard', clipboardDescriptor);
    } else {
      delete (navigator as { clipboard?: unknown }).clipboard;
    }
  });

  function setup() {
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement, host: fixture.componentInstance };
  }

  it('says the mail could not be sent, in a warning, with the reason', () => {
    const { el } = setup();

    const alert = el.querySelector('gbt-alert .gbt-alert')!;
    expect(alert.getAttribute('data-variant')).toBe('warning');
    expect(text(alert)).toContain("Le mail n'a pas pu être envoyé");
    expect(text(el.querySelector('.link-mail-failed__reason'))).toBe('connection refused');
  });

  it('shows the activation link and how long it lives, named after the invitee', () => {
    const { el } = setup();

    expect(text(el.querySelector('gbt-copy-field code'))).toBe('http://localhost:4200/activate#token=abc123');
    expect(text(el)).toContain('Transmettez ce lien à bob');
    expect(text(el)).toContain('24 heures');
  });

  it('copies the link with a button that names it', async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    const { fixture, el } = setup();

    const copy = el.querySelector<HTMLButtonElement>('button[aria-label="Copier le lien d\'activation"]')!;
    expect(copy).not.toBeNull();
    copy.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(writeText).toHaveBeenCalledWith('http://localhost:4200/activate#token=abc123');
    expect(text(el.querySelector('.gbt-copy-button__status'))).toBe('Copié');
  });

  it('selects the link for a manual copy when the clipboard is unavailable', async () => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
    const { fixture, el } = setup();

    el.querySelector<HTMLButtonElement>('button[aria-label="Copier le lien d\'activation"]')!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(text(el.querySelector('.gbt-copy-button__status'))).toBe('Copie impossible, lien sélectionné');
    expect(window.getSelection()?.toString()).toBe('http://localhost:4200/activate#token=abc123');
  });

  it('copies through the legacy copy command when the page has no clipboard API (plain-HTTP instance)', async () => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
    // jsdom has no `execCommand`, so the stub is an own property of `document` that is deleted afterwards.
    const execCommand = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, 'execCommand', { configurable: true, writable: true, value: execCommand });
    try {
      const { fixture, el } = setup();

      el.querySelector<HTMLButtonElement>('button[aria-label="Copier le lien d\'activation"]')!.click();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(execCommand).toHaveBeenCalledWith('copy');
      expect(text(el.querySelector('.gbt-copy-button__status'))).toBe('Copié');
    } finally {
      delete (document as { execCommand?: unknown }).execCommand;
    }
  });

  it('omits the reason line when the server gave none', () => {
    const { fixture, el, host } = setup();
    host.emailError.set(undefined);
    fixture.detectChanges();

    expect(el.querySelector('.link-mail-failed__reason')).toBeNull();
    expect(text(el)).toContain("Le mail n'a pas pu être envoyé");
  });
});

describe('LinkMailFailed (dismissible, without link)', () => {
  it('is one warning alert, dismissible on request, and not otherwise', () => {
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.gbt-alert__close')).toBeNull();
    expect(fixture.nativeElement.querySelectorAll('gbt-alert')).toHaveLength(1);

    const dismissible = TestBed.createComponent(Host);
    dismissible.componentInstance.dismissible = true;
    dismissible.detectChanges();
    const close = dismissible.nativeElement.querySelector('.gbt-alert__close') as HTMLButtonElement;
    expect(close.getAttribute('aria-label')).toBe('Fermer');
    close.click();
    expect(dismissible.componentInstance.dismissedCount).toBe(1);
  });

  it('points to "Renvoyer l\'invitation" when there is no link', () => {
    const fixture = TestBed.createComponent(Host);
    fixture.componentInstance.activationUrl.set(undefined);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('gbt-copy-field')).toBeNull();
    expect(text(el)).toContain("Renvoyer l'invitation");
  });

  it('can take the focus (the page moves it here when the alert appears)', () => {
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    fixture.debugElement.children[0].componentInstance.focus();
    expect(document.activeElement).toBe(fixture.nativeElement.querySelector('.link-mail-failed'));
  });
});

describe('LinkMailFailed (an admin password reset)', () => {
  @Component({
    imports: [LinkMailFailed],
    template: `<fg-link-mail-failed kind="password-reset" username="bob" emailError="connection refused" [url]="url()" />`,
  })
  class ResetHost {
    url = signal<string | undefined>('http://localhost:4200/reset-password#token=abc123');
  }

  it('shows the reset link, its one-hour lifetime and how to get another, with a copy button that names it', () => {
    const fixture = TestBed.createComponent(ResetHost);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;

    expect(text(el)).toContain("Le mail n'a pas pu être envoyé");
    expect(text(el.querySelector('gbt-copy-field code'))).toBe('http://localhost:4200/reset-password#token=abc123');
    expect(text(el.querySelector('.link-mail-failed__help'))).toBe(
      'Transmettez ce lien à bob : il est valable 1 heure. Il ne sera plus affiché ensuite — « Réinitialiser le mot de passe » en génère un nouveau.',
    );
    expect(el.querySelector('button[aria-label="Copier le lien de réinitialisation"]')).not.toBeNull();
    expect(text(el)).not.toContain('invitation');
  });

  it('points to the reset again when there is no link', () => {
    const fixture = TestBed.createComponent(ResetHost);
    fixture.componentInstance.url.set(undefined);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;

    expect(el.querySelector('gbt-copy-field')).toBeNull();
    expect(text(el.querySelector('.link-mail-failed__help'))).toBe('Utilisez à nouveau « Réinitialiser le mot de passe » pour obtenir un lien pour bob.');
  });
});
