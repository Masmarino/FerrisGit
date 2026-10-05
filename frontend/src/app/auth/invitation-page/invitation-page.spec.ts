import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from '@angular/router';
import { InvitationPage } from './invitation-page';

const ACTIVATE = '/api/auth/activate';
const TOKEN = 'ab12'.repeat(16);
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

const field = (el: HTMLElement, label: string) => {
  const labelEl = Array.from(el.querySelectorAll('label')).find((l) => text(l) === label);
  return labelEl ? el.querySelector<HTMLInputElement>(`[id="${labelEl.getAttribute('for')}"]`) : null;
};
const type = (input: HTMLInputElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input'));
};

describe('InvitationPage', () => {
  async function setup(fragment: string | null = `token=${TOKEN}`) {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
        {
          provide: ActivatedRoute,
          useValue: {
            snapshot: { fragment, queryParamMap: convertToParamMap({}) },
          },
        },
      ],
    });
    const http = TestBed.inject(HttpTestingController);
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockResolvedValue(true);
    const fixture = TestBed.createComponent(InvitationPage);
    const refresh = async () => {
      fixture.detectChanges();
      await fixture.whenStable();
      await settle();
      fixture.detectChanges();
    };
    await refresh();
    return {
      http,
      navigate,
      refresh,
      el: fixture.nativeElement as HTMLElement,
    };
  }
  type Ctx = Awaited<ReturnType<typeof setup>>;

  async function submit(ctx: Ctx, username = 'Marie') {
    type(field(ctx.el, "Nom d'utilisateur")!, username);
    type(field(ctx.el, 'Nouveau mot de passe')!, 'a-long-password');
    type(field(ctx.el, 'Confirmez le mot de passe')!, 'a-long-password');
    await ctx.refresh();
    ctx.el.querySelector('form')!.dispatchEvent(new Event('submit'));
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('asks the invitee for a username before the passwords, in French', async () => {
    const { el } = await setup();
    const fields = Array.from(el.querySelectorAll('input')).map((input) => input.id.split('-').pop());

    expect(fields).toEqual(['username', 'password', 'confirmation']);
    expect(text(el.querySelector('h1'))).toBe('Activez votre compte');
    expect(text(el)).toContain("Choisissez votre nom d'utilisateur et votre mot de passe.");
    expect(field(el, "Nom d'utilisateur")?.getAttribute('autocomplete')).toBe('username');
  });

  it('takes the token out of the URL once read', async () => {
    const { navigate } = await setup();

    expect(navigate).toHaveBeenCalledExactlyOnceWith('/invitation', {
      replaceUrl: true,
    });
  });

  it('posts the token, the chosen username and the password, then says the account is active', async () => {
    const ctx = await setup();

    await submit(ctx);
    const req = ctx.http.expectOne(ACTIVATE);
    expect(req.request.body).toEqual({
      token: TOKEN,
      password: 'a-long-password',
      username: 'Marie',
    });
    req.flush(null, { status: 204, statusText: 'No Content' });
    await ctx.refresh();

    expect(text(ctx.el.querySelector('h1'))).toBe('Votre compte est activé');
  });

  it('keeps the form and focuses the username when it is already taken', async () => {
    const ctx = await setup();

    await submit(ctx, 'alice');
    ctx.http.expectOne(ACTIVATE).flush({ error: 'username already taken' }, { status: 409, statusText: 'Conflict' });
    await ctx.refresh();

    expect(text(ctx.el.querySelector('h1'))).toBe('Activez votre compte');
    expect(text(ctx.el.querySelector('gbt-alert'))).toContain("Ce nom d'utilisateur est déjà utilisé");
    expect(document.activeElement).toBe(field(ctx.el, "Nom d'utilisateur"));
  });

  it('shows the dead link, without any request, when the link has no token', async () => {
    const { el } = await setup(null);

    expect(text(el.querySelector('h1'))).toBe('Ce lien ne fonctionne pas');
  });
});
