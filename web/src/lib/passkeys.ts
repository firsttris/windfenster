/**
 * The passkey ceremonies against the server (crates/wf-api/src/auth): ask for options, let the
 * browser make or use a passkey (@simplewebauthn/browser), send the answer back. Signing in is
 * offered in the name field by the browser itself while the page is open (autofill), as in
 * firsttris/haben's login, or asked for with a button.
 */
import {
  browserSupportsWebAuthn,
  browserSupportsWebAuthnAutofill,
  type PublicKeyCredentialCreationOptionsJSON,
  type PublicKeyCredentialRequestOptionsJSON,
  startAuthentication,
  startRegistration,
  WebAuthnAbortService,
} from '@simplewebauthn/browser';
import type { Account } from '@/bindings/Account';
import type { PasskeyOptions } from '@/bindings/PasskeyOptions';
import { send } from './api';

/** webauthn-rs wraps the options as the WebAuthn call takes them: `{ publicKey: … }`. */
function publicKey<T>(o: PasskeyOptions): T {
  return (o.options as { publicKey: T }).publicKey;
}

export function canUsePasskeys(): boolean {
  return browserSupportsWebAuthn();
}

/** A new account with a passkey made on this device; signed in afterwards. */
export async function signUp(name: string, invite?: string): Promise<Account> {
  const o = await send<PasskeyOptions>('POST', '/auth/signup', { name, invite });
  const credential = await startRegistration({
    optionsJSON: publicKey<PublicKeyCredentialCreationOptionsJSON>(o),
  });
  return send<Account>('POST', '/auth/signup/finish', { ceremony: o.ceremony, credential });
}

/** Signs in with a passkey of this site: the browser's own dialog, or (`autofill`) the passkeys
 * it offers in the field marked `autocomplete="username webauthn"` until one is picked. */
export async function signIn(autofill = false): Promise<Account> {
  const o = await send<PasskeyOptions>('POST', '/auth/signin', { autofill });
  const credential = await startAuthentication({
    optionsJSON: publicKey<PublicKeyCredentialRequestOptionsJSON>(o),
    useBrowserAutofill: autofill,
  });
  return send<Account>('POST', '/auth/signin/finish', { ceremony: o.ceremony, credential });
}

/** Whether the browser offers passkeys in a form field. */
export function canAutofill(): Promise<boolean> {
  return browserSupportsWebAuthnAutofill();
}

/** Ends a waiting autofill, e.g. when the page is left or a button is used instead. */
export function stopAutofill() {
  WebAuthnAbortService.cancelCeremony();
}

/** Another passkey for the signed-in account (another device or password manager). */
export async function addPasskey(): Promise<Account> {
  const o = await send<PasskeyOptions>('POST', '/auth/passkeys');
  const credential = await startRegistration({
    optionsJSON: publicKey<PublicKeyCredentialCreationOptionsJSON>(o),
  });
  return send<Account>('POST', '/auth/passkeys/finish', { ceremony: o.ceremony, credential });
}

/** The user closed the browser's dialog or it timed out: nothing to report. */
export function cancelled(e: unknown): boolean {
  return e instanceof Error && (e.name === 'NotAllowedError' || e.name === 'AbortError');
}
