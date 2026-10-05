import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { GbtInput } from '@masmarino/gabarit/input';
import { SecretReveal } from '@masmarino/gabarit/secret-reveal';
import { FieldSaveState } from '../field-save-state';
import { SettingsEditor } from '../settings-editor';

type TokenAction = 'generate' | 'remove';

/** 256 random bits in hexadecimal: 64 characters. */
function randomToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

/**
 * The registration token runners present to `POST /api/runner/register`. The server only keeps its hash. Generating
 * and removing it are actions, saved at once after a confirmation; a token typed by hand goes with the section's
 * "Enregistrer".
 */
@Component({
  selector: 'fg-runner-registration-token',
  standalone: true,
  imports: [FormsModule, Badge, Button, ConfirmDangerModal, GbtInput, SecretReveal, FieldSaveState],
  templateUrl: './runner-registration-token.html',
  styleUrl: './runner-registration-token.scss',
})
export class RunnerRegistrationToken {
  protected editor = inject(SettingsEditor);

  protected configured = computed(() => this.editor.saved().runnerRegistrationTokenConfigured);
  /** A token generated here, shown once since the server only keeps its hash. */
  protected generatedToken = signal<string | null>(null);
  protected pendingAction = signal<TokenAction | null>(null);

  /** Replacing an existing token asks first, since the old one stops working for new registrations. */
  protected askGenerate(): void {
    if (this.configured()) {
      this.pendingAction.set('generate');
    } else {
      this.generate();
    }
  }

  protected askRemove(): void {
    this.pendingAction.set('remove');
  }

  protected confirmAction(): void {
    const action = this.pendingAction();
    this.pendingAction.set(null);
    if (action === 'generate') {
      this.generate();
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

  /** Saved at once, then shown this once, since the server only keeps its hash. */
  private generate(): void {
    const token = randomToken();
    this.editor.runnerTokenDraft.set('');
    this.generatedToken.set(null);
    this.editor.save(
      'runnerRegistrationToken',
      { runnerRegistrationToken: token },
      () => undefined,
      () => this.generatedToken.set(token),
    );
  }
}
