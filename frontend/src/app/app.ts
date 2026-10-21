import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { LocaleScope } from './shared/i18n/locale-scope';
import { activeLanguage } from './shared/i18n/translator';

@Component({
  selector: 'fg-root',
  standalone: true,
  imports: [RouterOutlet, LocaleScope],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  protected readonly language = activeLanguage;
}
