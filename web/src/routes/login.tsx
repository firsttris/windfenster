import { useQueryClient } from '@tanstack/react-query';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { Fingerprint, KeyRound, Mail } from 'lucide-react';
import { type ComponentProps, type FormEvent, useEffect, useState } from 'react';
import type { Account } from '@/bindings/Account';
import { Page } from '@/components/page';
import { isApiError, send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { canAutofill, cancelled, canUsePasskeys, signIn, signUp, stopAutofill } from '@/lib/passkeys';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Segmented } from '@/ui/segmented';

interface LoginSearch {
  /** Where to go once signed in (a path of this site) */
  redirect?: string;
}

export const Route = createFileRoute('/login')({
  validateSearch: (s: Record<string, unknown>): LoginSearch => ({
    redirect: typeof s.redirect === 'string' && s.redirect.startsWith('/') ? s.redirect : undefined,
  }),
  component: LoginPage,
});

type Tab = 'in' | 'new';

const FIELD =
  'h-11 rounded-md border bg-background px-3 font-normal text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60';

function Field({ label, ...input }: { label: string } & ComponentProps<'input'>) {
  return (
    <label className="flex flex-col gap-1.5 font-medium text-sm">
      {label}
      <input className={FIELD} {...input} />
    </label>
  );
}

/** Sign in or sign up: a passkey first (offered by the browser in the e-mail field as soon as
 * the page is open), e-mail and password as the other way, as in firsttris/haben's login. */
function LoginPage() {
  useTitle(m.title_login());
  const { redirect } = Route.useSearch();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<Tab>('in');
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [withPassword, setWithPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const passkeys = canUsePasskeys();

  const done = async (account: Account) => {
    queryClient.setQueryData(['me'], account);
    await queryClient.invalidateQueries({ queryKey: ['favorites'] });
    await navigate({ to: redirect ?? '/' });
  };

  const attempt = async (ceremony: () => Promise<Account>) => {
    stopAutofill();
    setBusy(true);
    setError(null);
    try {
      await done(await ceremony());
    } catch (e) {
      if (!cancelled(e)) setError(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  // While the page is open the browser offers this site's passkeys in the e-mail field.
  // biome-ignore lint/correctness/useExhaustiveDependencies: started once per visit
  useEffect(() => {
    let live = true;
    void canAutofill().then((ok) => {
      if (!ok || !live) return;
      signIn(true)
        .then((account) => {
          if (live) void done(account);
        })
        // A browser that turns the offer down says nothing to the user; only the server's
        // answer (a rejected passkey) is shown.
        .catch((e) => {
          if (live && isApiError(e) && e.code) setError(describeError(e));
        });
    });
    return () => {
      live = false;
      stopAutofill();
    };
  }, []);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (tab === 'in') {
      void attempt(() => send<Account>('POST', '/auth/signin/password', { email, password }));
    } else if (withPassword || !passkeys) {
      void attempt(() => send<Account>('POST', '/auth/signup/password', { name, email, password }));
    } else {
      void attempt(() => signUp(name));
    }
  };

  const credentials = (
    <>
      <Field
        label={m.login_email()}
        name="email"
        type="email"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        // The browser offers passkeys here (autofill) before anything is typed.
        autoComplete="username webauthn"
        required
      />
      <Field
        label={m.login_password()}
        name="password"
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        autoComplete={tab === 'in' ? 'current-password' : 'new-password'}
        required
        minLength={tab === 'in' ? undefined : 10}
      />
    </>
  );

  return (
    <Page>
      <section className="mx-auto flex w-full max-w-md flex-col gap-6 py-6 sm:py-12">
        <div className="flex flex-col gap-2">
          <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.login_title()}</h1>
          <p className="m-0 text-subtle leading-relaxed">{m.login_lead()}</p>
        </div>
        <Segmented
          label={m.login_title()}
          value={tab}
          stretch
          options={[
            { value: 'in', label: m.login_haveAccount() },
            { value: 'new', label: m.login_noAccount() },
          ]}
          onChange={(t) => {
            setTab(t);
            setError(null);
          }}
        />
        {!passkeys && (
          <p role="alert" className="m-0 rounded-lg border p-3 text-sm">
            {m.login_unsupported()}
          </p>
        )}
        {tab === 'in' && passkeys && (
          <>
            <Button size="lg" disabled={busy} onClick={() => void attempt(() => signIn(false))}>
              <Fingerprint aria-hidden="true" />
              {m.login_signIn()}
            </Button>
            <div className="flex items-center gap-3 text-muted-foreground text-xs">
              <span className="h-px flex-1 bg-line" />
              {m.login_or()}
              <span className="h-px flex-1 bg-line" />
            </div>
          </>
        )}
        <form onSubmit={submit} className="flex flex-col gap-3">
          {tab === 'new' && (
            <Field
              label={m.login_name()}
              name="name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoComplete="nickname"
              required
              maxLength={60}
              placeholder={m.login_namePlaceholder()}
            />
          )}
          {(tab === 'in' || withPassword || !passkeys) && credentials}
          {tab === 'in' ? (
            <Button type="submit" variant="outline" size="lg" disabled={busy}>
              <Mail aria-hidden="true" />
              {m.login_passwordSignIn()}
            </Button>
          ) : withPassword || !passkeys ? (
            <>
              <Button type="submit" size="lg" disabled={busy}>
                <Mail aria-hidden="true" />
                {m.login_passwordCreate()}
              </Button>
              <p className="m-0 text-muted-foreground text-xs leading-relaxed">{m.login_passwordHint()}</p>
              {passkeys && (
                <Button type="button" variant="link" className="w-fit px-0" onClick={() => setWithPassword(false)}>
                  {m.login_usePasskey()}
                </Button>
              )}
            </>
          ) : (
            <>
              <Button type="submit" size="lg" disabled={busy || !name.trim()}>
                <KeyRound aria-hidden="true" />
                {m.login_create()}
              </Button>
              <p className="m-0 text-muted-foreground text-xs leading-relaxed">{m.login_createHint()}</p>
              <Button type="button" variant="link" className="w-fit px-0" onClick={() => setWithPassword(true)}>
                {m.login_withPassword()}
              </Button>
            </>
          )}
        </form>
        {error && (
          <p role="alert" className="m-0 text-destructive text-sm">
            {error}
          </p>
        )}
      </section>
    </Page>
  );
}
