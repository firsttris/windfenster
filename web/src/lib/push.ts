/**
 * Push messages in this browser: the service worker (`public/sw.js`) shows them; the browser's
 * subscription goes to the server, which signs and encrypts each message for it. On an iPhone
 * or iPad only a site added to the home screen may take them (iOS 16.4 and later).
 */
import type { PushSubscription as Target } from '@/bindings/PushSubscription';
import { send } from './api';

export type PushSupport = 'ok' | 'home-screen' | 'unsupported';

export function pushSupport(): PushSupport {
  const ios =
    /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  const standalone = window.matchMedia('(display-mode: standalone)').matches;
  if (ios && !standalone) return 'home-screen';
  return 'serviceWorker' in navigator && 'PushManager' in window && 'Notification' in window ? 'ok' : 'unsupported';
}

async function registration(): Promise<ServiceWorkerRegistration> {
  return (await navigator.serviceWorker.getRegistration('/')) ?? navigator.serviceWorker.register('/sw.js');
}

/** This browser's subscription, if it has one */
export async function currentPush(): Promise<PushSubscription | null> {
  if (pushSupport() !== 'ok') return null;
  const reg = await navigator.serviceWorker.getRegistration('/');
  return (await reg?.pushManager.getSubscription()) ?? null;
}

/** The server's key (base64url) as `applicationServerKey` wants it */
export function keyBytes(key: string): Uint8Array<ArrayBuffer> {
  const b64 = (key + '='.repeat((4 - (key.length % 4)) % 4)).replace(/-/g, '+').replace(/_/g, '/');
  const raw = atob(b64);
  const out = new Uint8Array(new ArrayBuffer(raw.length));
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

/** What the server keeps of a subscription */
export function targetOf(sub: PushSubscriptionJSON): Target | null {
  const { endpoint, keys } = sub;
  return endpoint && keys?.p256dh && keys.auth ? { endpoint, p256dh: keys.p256dh, auth: keys.auth } : null;
}

/** Asks for permission, subscribes this browser and tells the server; false when refused. */
export async function enablePush(key: string): Promise<boolean> {
  if ((await Notification.requestPermission()) !== 'granted') return false;
  const reg = await registration();
  await navigator.serviceWorker.ready;
  const sub =
    (await reg.pushManager.getSubscription()) ??
    (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: keyBytes(key) }));
  const target = targetOf(sub.toJSON());
  if (!target) return false;
  await send('POST', '/push/subscriptions', target);
  return true;
}

export async function disablePush(): Promise<void> {
  const sub = await currentPush();
  if (!sub) return;
  await send('DELETE', '/push/subscriptions', { endpoint: sub.endpoint });
  await sub.unsubscribe();
}
