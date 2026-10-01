import type { TotpQrRenderOptions, TotpQrRenderer } from '@masmarino/gabarit';

type QrCodeModule = typeof import('qrcode');

/** The `toDataURL` of the `qrcode` module, whether the bundler hands the CommonJS package over as a named export or only under `default`. */
export function qrToDataUrl(module: QrCodeModule): QrCodeModule['toDataURL'] {
  return module.toDataURL ?? (module as unknown as { default: QrCodeModule }).default.toDataURL;
}

/**
 * Imported lazily because only the enrolment screens need it. Security: `text` carries the user's raw TOTP secret.
 * It is encoded locally into a `data:` URL, with no network call and no hosted QR service, and is never logged.
 */
export const renderTotpQr: TotpQrRenderer = (text: string, options: TotpQrRenderOptions) =>
  import('qrcode').then((module) => qrToDataUrl(module)(text, options));
