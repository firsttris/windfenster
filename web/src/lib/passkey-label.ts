/** What a passkey is called on the account page: the user's own name, else the browser and
 * system it was added with (the server reads them off the User-Agent), else just "Passkey". */
import type { PasskeySummary } from '@/bindings/PasskeySummary';
import { m } from '@/paraglide/messages';

export function passkeyLabel({ name, browser, os }: Pick<PasskeySummary, 'name' | 'browser' | 'os'>): string {
  if (name) return name;
  if (browser && os) return m.account_passkeyDevice({ browser, os });
  return browser ?? os ?? m.account_passkeyUnknown();
}
