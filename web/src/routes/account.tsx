import { useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute, Link, Navigate, useNavigate } from '@tanstack/react-router';
import { KeyRound, LogOut, Sailboat, Trash2 } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import type { Account } from '@/bindings/Account';
import { NotificationsCard } from '@/components/notifications-card';
import { Loading, Page } from '@/components/page';
import { meQuery, send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { addPasskey, cancelled } from '@/lib/passkeys';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card, CardHeader, CardTitle } from '@/ui/card';

export const Route = createFileRoute('/account')({
  component: AccountPage,
});

function AccountPage() {
  useTitle(m.title_account());
  const me = useQuery(meQuery);
  if (me.isPending) return <Loading />;
  if (!me.data) return <Navigate to="/login" search={{ redirect: '/account' }} />;
  return <Signed account={me.data} />;
}

function Signed({ account }: { account: Account }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [name, setName] = useState(account.name);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setSaved(false);
    try {
      await action();
    } catch (e) {
      if (!cancelled(e)) setError(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  const leave = async () => {
    queryClient.setQueryData(['me'], null);
    queryClient.removeQueries({ queryKey: ['favorites'] });
    await navigate({ to: '/' });
  };

  const rename = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      queryClient.setQueryData(['me'], await send<Account>('PATCH', '/me', { name }));
      setSaved(true);
    });
  };

  return (
    <Page>
      <section className="mx-auto flex w-full max-w-xl flex-col gap-5 py-6">
        <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.account_title()}</h1>
        <Card>
          <CardHeader>
            <CardTitle>{m.account_name()}</CardTitle>
          </CardHeader>
          <form onSubmit={rename} className="flex gap-2">
            <input
              aria-label={m.account_name()}
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={60}
              required
              className="h-10 min-w-0 flex-1 rounded-md border bg-background px-3 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            />
            <Button type="submit" variant="outline" disabled={busy || !name.trim() || name.trim() === account.name}>
              {m.account_save()}
            </Button>
          </form>
          {saved && <p className="m-0 text-muted-foreground text-sm">{m.account_saved()}</p>}
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>{m.account_passkeys()}</CardTitle>
          </CardHeader>
          <p className="m-0 text-sm text-subtle">{m.account_passkeysCount({ count: account.passkeys })}</p>
          <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_passkeysHint()}</p>
          <Button
            variant="outline"
            className="w-fit"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                queryClient.setQueryData(['me'], await addPasskey());
              })
            }
          >
            <KeyRound aria-hidden="true" />
            {m.account_addPasskey()}
          </Button>
        </Card>
        <PasswordCard account={account} />
        <NotificationsCard account={account} />
        <Card>
          <CardHeader>
            <CardTitle>{m.gear_title()}</CardTitle>
          </CardHeader>
          <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_gearHint()}</p>
          <Button variant="outline" className="w-fit" asChild>
            <Link to="/gear">
              <Sailboat aria-hidden="true" />
              {m.gear_edit()}
            </Link>
          </Button>
        </Card>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await send('POST', '/auth/signout');
                await leave();
              })
            }
          >
            <LogOut aria-hidden="true" />
            {m.account_signOut()}
          </Button>
          {confirming ? (
            <Button
              variant="outline"
              className="border-destructive/50 text-destructive"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await send('DELETE', '/me');
                  await leave();
                })
              }
            >
              <Trash2 aria-hidden="true" />
              {m.account_deleteConfirm()}
            </Button>
          ) : (
            <Button variant="ghost" disabled={busy} onClick={() => setConfirming(true)}>
              <Trash2 aria-hidden="true" />
              {m.account_delete()}
            </Button>
          )}
        </div>
        {confirming && <p className="m-0 text-muted-foreground text-sm">{m.account_deleteHint()}</p>}
        {error && (
          <p role="alert" className="m-0 text-destructive text-sm">
            {error}
          </p>
        )}
      </section>
    </Page>
  );
}

const FIELD =
  'h-10 rounded-md border bg-background px-3 font-normal text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60';

/** E-mail and password, to sign in without a passkey; changing them asks for the password. */
function PasswordCard({ account }: { account: Account }) {
  const queryClient = useQueryClient();
  const [email, setEmail] = useState(account.email ?? '');
  const [current, setCurrent] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setMessage(null);
    try {
      const body = { email, password, current: account.email ? current : null };
      queryClient.setQueryData(['me'], await send<Account>('PUT', '/me/password', body));
      setCurrent('');
      setPassword('');
      setMessage({ error: false, text: m.account_saved() });
    } catch (err) {
      setMessage({ error: true, text: describeError(err) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card>
      <CardHeader>
        <CardTitle>{m.account_password()}</CardTitle>
      </CardHeader>
      {!account.email && (
        <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_passwordNone()}</p>
      )}
      <form onSubmit={save} className="flex flex-col gap-2.5">
        <label className="flex flex-col gap-1 font-medium text-sm">
          {m.login_email()}
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            autoComplete="username"
            required
            className={FIELD}
          />
        </label>
        {account.email && (
          <label className="flex flex-col gap-1 font-medium text-sm">
            {m.account_current()}
            <input
              type="password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
              autoComplete="current-password"
              required
              className={FIELD}
            />
          </label>
        )}
        <label className="flex flex-col gap-1 font-medium text-sm">
          {m.account_newPassword()}
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            minLength={10}
            required
            className={FIELD}
          />
        </label>
        <Button type="submit" variant="outline" className="w-fit" disabled={busy}>
          {m.account_setPassword()}
        </Button>
      </form>
      {message && (
        <p
          role={message.error ? 'alert' : undefined}
          className={message.error ? 'm-0 text-destructive text-sm' : 'm-0 text-muted-foreground text-sm'}
        >
          {message.text}
        </p>
      )}
    </Card>
  );
}
