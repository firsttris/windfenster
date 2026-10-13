/** The account's ways of being told: push on this device, the e-mail address (confirmed by a
 * link before alarms go there), and a test message to see that they arrive. */
import { useQuery } from '@tanstack/react-query';
import { MailCheck, Send } from 'lucide-react';
import { useState } from 'react';
import type { Account } from '@/bindings/Account';
import type { Reached } from '@/bindings/Reached';
import { pushInfoQuery, send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card, CardHeader, CardTitle } from '@/ui/card';
import { PushDevice } from './push-device';

export function NotificationsCard({ account }: { account: Account }) {
  const info = useQuery(pushInfoQuery);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const confirm = async () => {
    setBusy(true);
    setMessage(null);
    try {
      await send('POST', '/me/email/confirm');
      setMessage({ error: false, text: m.notify_confirmationSent() });
    } catch (e) {
      setMessage({ error: true, text: describeError(e) });
    } finally {
      setBusy(false);
    }
  };
  const test = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const r = await send<Reached>('POST', '/alarms/test');
      const text =
        r.browsers === 0 && !r.email
          ? m.notify_testNone()
          : m.notify_testSent({ browsers: r.browsers, email: r.email ? m.notify_andEmail() : '' });
      setMessage({ error: r.browsers === 0 && !r.email, text });
    } catch (e) {
      setMessage({ error: true, text: describeError(e) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card>
      <CardHeader>
        <CardTitle>{m.notify_title()}</CardTitle>
      </CardHeader>
      <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.notify_lead()}</p>
      <PushDevice />
      <p className="m-0 text-sm">
        {!info.data?.email
          ? m.alarm_emailOff()
          : !account.email
            ? m.alarm_emailNone()
            : account.emailConfirmed
              ? m.notify_email({ email: account.email })
              : m.notify_emailUnconfirmed({ email: account.email })}
      </p>
      <div className="flex flex-wrap gap-2">
        {info.data?.email && account.email && !account.emailConfirmed && (
          <Button variant="outline" className="w-fit" disabled={busy} onClick={() => void confirm()}>
            <MailCheck aria-hidden="true" />
            {m.notify_sendConfirmation()}
          </Button>
        )}
        <Button variant="outline" className="w-fit" disabled={busy} onClick={() => void test()}>
          <Send aria-hidden="true" />
          {m.notify_test()}
        </Button>
      </div>
      {message && (
        <p
          role={message.error ? 'alert' : 'status'}
          className={message.error ? 'm-0 text-destructive text-sm' : 'm-0 text-muted-foreground text-sm'}
        >
          {message.text}
        </p>
      )}
    </Card>
  );
}
