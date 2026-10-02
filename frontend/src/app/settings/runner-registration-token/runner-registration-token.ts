import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Badge, Button, ConfirmDangerModal, GbtInput, SecretReveal } from '@masmarino/gabarit';
import { FieldSaveState } from '../field-save-state';
import { SettingsEditor } from '../settings-editor';

type TokenAction = 'generate' | 'remove';

/** 256 random bits in hexadecimal: 64 characters. */
function randomToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** The registration token runners present to `POST /api/runner/register`. The server only keeps its hash. */
@Component({
  selector: 'fg-runner-registration-token',
  standalone: true,
  imports: [FormsModule, Badge, Button, ConfirmDangerModal, GbtInput, SecretReveal, FieldSaveState],
  templateUrl: './runner-registration-token.html',
  styleUrl: './runner-registration-token.scss',
})
export class RunnerRegistrationToken {
  private editor = inject(SettingsEditor);

  protected configured = computed(() => this.editor.saved().runnerRegistrationTokenConfigured);
  /** What the admin is typing as the new token. It never comes back from the server. */
  protected tokenDraft = signal('');
  /** A token generated here, shown once since the server only keeps its hash. */
  protected generatedToken = signal<string | null>(null);
  protected pendingAction = signal<TokenAction | null>(null);

  /** Called on blur. An empty field saves nothing: that's not how the token is removed (see `askRemove`). */
  protected setToken(value: string): void {
    const token = value.trim();
    if (token !== '') {
      this.save(token, false);
    }
  }

  /** Replacing an existing token asks first, since the old one stops working for new registrations. */
  protected askGenerate(): void {
    if (this.configured()) {
      this.pendingAction.set('generate');
    } else {
      this.save(randomToken(), true);
    }
  }

  protected askRemove(): void {
    this.pendingAction.set('remove');
  }

  protected confirmAction(): void {
    const action = this.pendingAction();
    this.pendingAction.set(null);
    if (action === 'generate') {
      this.save(randomToken(), true);
    } else if (action === 'remove') {
      this.generatedToken.set(null);
      this.editor.save('runnerRegistrationToken', { runnerRegistrationToken: null }, () => undefined);
    }
  }

  protected cancelAction(): void {
    this.pendingAction.set(null);
  }

  protected dismissGenerated(): void {
    this.generatedToken.set(null);
  }

  private save(token: string, reveal: boolean): void {
    this.tokenDraft.set('');
    this.generatedToken.set(null);
    this.editor.save(
      'runnerRegistrationToken',
      { runnerRegistrationToken: token },
      () => undefined,
      () => {
        if (reveal) {
          this.generatedToken.set(token);
        }
      },
    );
  }
}
