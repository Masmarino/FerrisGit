import { afterNextRender, Component, computed, DestroyRef, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { TranslocoPipe } from '@jsverse/transloco';
import { AUTH_PORT_ERROR_BODIES, classifyPasskeyError, getPasskeyAssertion, isAuthPortError, passkeysSupported, portErrorMessage } from '@masmarino/gabarit/auth';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { GbtInput } from '@masmarino/gabarit/input';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { from, Subscription, switchMap } from 'rxjs';
import { describeScope } from '../../api-tokens/token-scopes';
import { PageTitleService } from '../../shell/page-title.service';
import { TranslocoCountPipe } from '../../shared/i18n/transloco-count.pipe';
import { t } from '../../shared/i18n/translator';
import { DeviceLoginService, DeviceProof, DeviceRequest } from './device-login.service';

type View = 'enter' | 'loading' | 'ready' | 'notFound' | 'rateLimited' | 'loadFailed' | 'approved' | 'denied';
type Factor = 'code' | 'backup' | 'passkey';

interface ApprovalError {
  message: string;
  variant: 'error' | 'info';
}

/** The code as a terminal shows it, or null. The server ignores case and dash; nothing else reaches a request path. */
function normalizeUserCode(input: string): string | null {
  const compact = input.toUpperCase().replace(/[\s-]/g, '');
  return /^[A-Z0-9]{8}$/.test(compact) ? `${compact.slice(0, 4)}-${compact.slice(4)}` : null;
}

/**
 * Confirms the sign-in of a terminal (`/account/device?code=ABCD-EFGH`). The code lives in the URL, which a terminal
 * prints as a ready-made link, so anyone can send that link: the page says so, and asks for the second factor again.
 */
@Component({
  selector: 'fg-device-page',
  standalone: true,
  imports: [TranslocoPipe, TranslocoCountPipe, FormsModule, Alert, Button, Card, GbtInput, PageHeader, PageLayout, SkeletonList],
  templateUrl: './device-page.html',
  styleUrl: './device-page.scss',
})
export class DevicePage implements OnInit {
  private service = inject(DeviceLoginService);
  private route = inject(ActivatedRoute);
  private router = inject(Router);
  private pageTitle = inject(PageTitleService);
  private injector = inject(Injector);
  private destroyRef = inject(DestroyRef);
  private host = inject(ElementRef);

  private factorField = viewChild('factorField', { read: ElementRef });
  /** The one running request or passkey prompt. Leaving the page drops it, and a prompt dropped approves nothing. */
  private running: Subscription | null = null;

  protected view = signal<View>('enter');
  protected request = signal<DeviceRequest | null>(null);
  protected typedCode = signal('');
  protected typedCodeError = signal<string | null>(null);

  protected code = signal('');
  protected backupCode = signal('');
  protected useBackupCode = signal(false);
  protected busy = signal<Factor | 'deny' | null>(null);
  protected error = signal<ApprovalError | null>(null);
  private browserHasPasskeys = signal(false);

  protected scopes = computed(() => (this.request()?.scopes ?? []).map(describeScope));
  protected canUsePasskey = computed(() => this.request()?.hasPasskey === true && this.browserHasPasskeys());
  protected hasTotp = computed(() => this.request()?.hasTotp === true);
  /** With no other factor usable here, the backup code is all that is left, and there is nothing to switch back to. */
  protected backupOnly = computed(() => !this.hasTotp() && !this.canUsePasskey());
  protected showBackupCode = computed(() => this.useBackupCode() || this.backupOnly());
  protected passkeyBlocked = computed(() => this.request()?.hasPasskey === true && !this.hasTotp() && !this.browserHasPasskeys());

  ngOnInit(): void {
    this.pageTitle.set(t('deviceLogin.title'));
    this.destroyRef.onDestroy(() => this.running?.unsubscribe());
    this.route.queryParamMap.pipe(takeUntilDestroyed(this.destroyRef)).subscribe((params) => this.open(params.get('code')));
  }

  private open(raw: string | null): void {
    this.running?.unsubscribe();
    this.busy.set(null);
    this.error.set(null);
    this.code.set('');
    this.backupCode.set('');
    this.useBackupCode.set(false);
    this.request.set(null);
    const userCode = normalizeUserCode(raw ?? '');
    if (userCode) {
      this.load(userCode);
    } else {
      this.view.set('enter');
      this.typedCode.set(raw ?? '');
      this.typedCodeError.set(raw ? t('deviceLogin.codeInvalid') : null);
    }
  }

  private load(userCode: string): void {
    this.view.set('loading');
    this.running = this.service.find(userCode).subscribe({
      next: (request) => {
        this.browserHasPasskeys.set(passkeysSupported());
        this.request.set(request);
        this.view.set('ready');
      },
      error: (err: unknown) => {
        const status = isAuthPortError(err) ? err.status : null;
        // On a 401 the interceptor is already on its way to the sign-in page, and it comes back here afterwards.
        if (status !== 401) {
          this.view.set(status === 404 ? 'notFound' : status === 429 ? 'rateLimited' : 'loadFailed');
        }
      },
    });
  }

  protected retry(): void {
    this.open(this.route.snapshot.queryParamMap.get('code'));
  }

  /** The URL holds the code, so a reload shows the same request, and `open` runs from there. */
  protected submitTypedCode(): void {
    const userCode = normalizeUserCode(this.typedCode());
    if (!userCode) {
      this.typedCodeError.set(t('deviceLogin.codeInvalid'));
      return;
    }
    this.typedCodeError.set(null);
    this.goTo(userCode);
  }

  protected enterAnotherCode(): void {
    this.goTo(null);
  }

  private goTo(code: string | null): void {
    void this.router.navigate([], { relativeTo: this.route, queryParams: { code } });
  }

  protected setTypedCode(value: string): void {
    this.typedCode.set(value);
    this.typedCodeError.set(null);
  }

  protected toggleBackupCode(): void {
    this.useBackupCode.update((use) => !use);
    this.error.set(null);
    this.focusFactorField();
  }

  /** Spaces are tolerated, as at sign-in: an authenticator app shows `123 456`. */
  protected approveWithCode(): void {
    if (this.showBackupCode()) {
      const backupCode = this.backupCode().trim();
      if (backupCode) {
        this.approve('backup', { backupCode });
      }
      return;
    }
    const code = this.code().replace(/\s/g, '');
    if (code) {
      this.approve('code', { code });
    }
  }

  private approve(factor: Factor, proof: DeviceProof): void {
    const userCode = this.request()?.userCode;
    if (!userCode || this.busy()) {
      return;
    }
    this.busy.set(factor);
    this.error.set(null);
    this.running = this.service.approve(userCode, proof).subscribe({
      next: () => this.finish('approved'),
      error: (err: unknown) => this.refused(factor, err),
    });
  }

  protected approveWithPasskey(): void {
    const userCode = this.request()?.userCode;
    if (!userCode || this.busy()) {
      return;
    }
    this.busy.set('passkey');
    this.error.set(null);
    this.running = this.service
      .startPasskey(userCode)
      .pipe(
        switchMap(({ challengeId, publicKey }) =>
          from(getPasskeyAssertion(publicKey)).pipe(switchMap((credential) => this.service.approve(userCode, { challengeId, credential }))),
        ),
      )
      .subscribe({
        next: () => this.finish('approved'),
        error: (err: unknown) => this.refused('passkey', err),
      });
  }

  protected deny(): void {
    const userCode = this.request()?.userCode;
    if (!userCode || this.busy()) {
      return;
    }
    this.busy.set('deny');
    this.error.set(null);
    this.running = this.service.deny(userCode).subscribe({
      next: () => this.finish('denied'),
      error: (err: unknown) => this.refused('deny', err),
    });
  }

  private finish(view: View): void {
    this.busy.set(null);
    this.code.set('');
    this.backupCode.set('');
    this.view.set(view);
    this.focusResult();
  }

  private refused(action: Factor | 'deny', err: unknown): void {
    this.busy.set(null);
    if (!isAuthPortError(err)) {
      this.error.set(this.passkeyProblem(err));
      return;
    }
    if (err.status === 404) {
      this.finish('notFound');
      return;
    }
    this.error.set({ message: this.refusalMessage(action, err.status, portErrorMessage(err)), variant: 'error' });
    if (err.status === 400 && (action === 'code' || action === 'backup')) {
      this.code.set('');
      this.backupCode.set('');
      this.focusFactorField();
    }
  }

  private refusalMessage(action: Factor | 'deny', status: number, reason: string | null): string {
    if (status === 429) {
      return t('deviceLogin.tooManyAttempts');
    }
    if (status === 503) {
      return t('deviceLogin.passkeysUnavailable');
    }
    if (status === 400 && reason === AUTH_PORT_ERROR_BODIES.invalidCode) {
      return action === 'passkey' ? t('deviceLogin.passkeyRefused') : action === 'backup' ? t('deviceLogin.wrongBackupCode') : t('deviceLogin.wrongCode');
    }
    if (status === 400 && reason?.includes('administrator')) {
      return t('deviceLogin.adminOnly');
    }
    return action === 'deny' ? t('deviceLogin.denyFailed') : t('deviceLogin.approveFailed');
  }

  /** What the browser's own passkey prompt answered; the server was not asked. */
  private passkeyProblem(err: unknown): ApprovalError {
    switch (classifyPasskeyError(err)) {
      case 'cancelled':
        return { message: t('deviceLogin.passkeyCancelled'), variant: 'info' };
      case 'unsupported':
        return { message: t('deviceLogin.passkeyUnsupported'), variant: 'error' };
      default:
        return { message: t('deviceLogin.approveFailed'), variant: 'error' };
    }
  }

  private focusFactorField(): void {
    afterNextRender(() => this.factorField()?.nativeElement.querySelector('input')?.focus(), { injector: this.injector });
  }

  /** The form the focus was in is gone. The result's heading takes it, from script only, and a screen reader reads it. */
  private focusResult(): void {
    afterNextRender(
      () => {
        const heading = (this.host.nativeElement as HTMLElement).querySelector<HTMLElement>('.device-page__result h2');
        if (heading) {
          heading.tabIndex = -1;
          heading.focus();
        }
      },
      { injector: this.injector },
    );
  }
}
