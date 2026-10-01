import { qrToDataUrl, renderTotpQr } from './totp-qr-renderer';

// jsdom has no canvas, so the package is mocked. What matters here is what it gets called with.
const toDataURL = vi.hoisted(() => vi.fn());
vi.mock('qrcode', () => ({ toDataURL }));

const OTPAUTH = 'otpauth://totp/FerrisGit:alice?secret=JBSWY3DPEHPK3PXP&issuer=FerrisGit';
const OPTIONS = { errorCorrectionLevel: 'M', margin: 1, width: 448 } as const;

describe('renderTotpQr', () => {
  beforeEach(() => {
    toDataURL.mockReset();
    toDataURL.mockResolvedValue('data:image/png;base64,QR');
  });

  it('encodes the otpauth URL with the bundled qrcode package, with the options it is given, into a data: URL', async () => {
    await expect(renderTotpQr(OTPAUTH, OPTIONS)).resolves.toBe('data:image/png;base64,QR');

    expect(toDataURL).toHaveBeenCalledExactlyOnceWith(OTPAUTH, OPTIONS);
  });

  it('never goes to the network with the secret', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const open = vi.spyOn(XMLHttpRequest.prototype, 'open');
    onTestFinished(() => {
      fetchSpy.mockRestore();
      open.mockRestore();
    });

    await renderTotpQr(OTPAUTH, OPTIONS);

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
  });

  it('fails when the package fails, so that the kit falls back to the key to type', async () => {
    toDataURL.mockRejectedValue(new Error('no canvas'));

    await expect(renderTotpQr(OTPAUTH, OPTIONS)).rejects.toThrow('no canvas');
  });
});

describe('qrToDataUrl', () => {
  it('takes the named export, or the one under `default` when the bundler only hands that over', () => {
    const named = vi.fn();
    const underDefault = vi.fn();

    expect(qrToDataUrl({ toDataURL: named } as never)).toBe(named);
    expect(qrToDataUrl({ default: { toDataURL: underDefault } } as never)).toBe(underDefault);
  });
});
