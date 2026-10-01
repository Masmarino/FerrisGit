import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { AuthLogo } from './auth-logo';

@Component({
  standalone: true,
  imports: [AuthLogo],
  template: `<div class="slot"><picture auth-logo fgAuthLogo></picture></div>`,
})
class Host {}

describe('AuthLogo', () => {
  it('is the picture itself (no wrapper): the dark-theme source, then the wordmark with its size and name', () => {
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const slot = (fixture.nativeElement as HTMLElement).querySelector('.slot')!;

    expect(slot.children).toHaveLength(1);
    const picture = slot.firstElementChild!;
    expect(picture.tagName).toBe('PICTURE');
    expect(picture.hasAttribute('auth-logo')).toBe(true);
    expect(Array.from(picture.children).map((child) => child.tagName)).toEqual(['SOURCE', 'IMG']);
    const source = picture.querySelector('source')!;
    expect(source.getAttribute('srcset')).toBe('Logo_horizontal_dark.png');
    expect(source.getAttribute('media')).toBe('(prefers-color-scheme: dark)');
    const img = picture.querySelector('img')!;
    expect(img.getAttribute('src')).toBe('Logo_horizontal.png');
    expect(img.getAttribute('alt')).toBe('FerrisGit');
    expect(img.getAttribute('width')).toBe('1762');
    expect(img.getAttribute('height')).toBe('592');
  });
});
