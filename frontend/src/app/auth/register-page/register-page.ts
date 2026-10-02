import { Component, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { AuthFooterLink, AuthRegister, Button } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';

@Component({
  selector: 'fg-register-page',
  standalone: true,
  imports: [AuthRegister, AuthLogo, AuthFooterLink, Button, RouterLink],
  template: `
    <gbt-auth-register (registered)="registered()" (signIn)="toSignIn()">
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/login">Se connecter</a>
    </gbt-auth-register>
  `,
  // The logo artwork has a transparent margin above the drawing; this pulls it back to the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class RegisterPage {
  private router = inject(Router);

  protected registered(): void {
    this.router.navigateByUrl('/repositories');
  }

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
