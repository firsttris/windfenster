import { useQueryClient } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { MailCheck } from 'lucide-react';
import { useState } from 'react';
import { Page } from '@/components/page';
import { send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card, CardHeader, CardTitle } from '@/ui/card';

interface ConfirmSearch {
  token?: string;
}

export const Route = createFileRoute('/confirm-email')({
  validateSearch: (s: Record<string, unknown>): ConfirmSearch => ({
    token: typeof s.token === 'string' ? s.token : undefined,
  }),
  component: ConfirmEmailPage,
});

/** Where the link in a confirmation e-mail leads. It confirms by a button, not on opening: mail
 * scanners open every link in a mail, and would otherwise confirm for whoever asked. */
function ConfirmEmailPage() {
  useTitle(m.title_confirmEmail());
  const { token } = Route.useSearch();
  const queryClient = useQueryClient();
  const [state, setState] = useState<'ready' | 'busy' | 'done'>('ready');
  const [error, setError] = useState<string | null>(token ? null : m.error_invalidLink());

  const confirm = async () => {
    setState('busy');
    setError(null);
    try {
      await send('POST', '/auth/email/confirm', { token });
      await queryClient.invalidateQueries({ queryKey: ['me'] });
      setState('done');
    } catch (e) {
      setError(describeError(e));
      setState('ready');
    }
  };

  return (
    <Page>
      <section className="mx-auto flex w-full max-w-xl flex-col gap-5 py-6">
        <Card>
          <CardHeader>
            <CardTitle>{m.confirm_title()}</CardTitle>
          </CardHeader>
          {state === 'done' ? (
            <>
              <p role="status" className="m-0 text-sm">
                {m.confirm_done()}
              </p>
              <Button variant="outline" className="w-fit" asChild>
                <Link to="/account">{m.confirm_toAccount()}</Link>
              </Button>
            </>
          ) : (
            <>
              <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.confirm_lead()}</p>
              <Button className="w-fit" disabled={!token || state === 'busy'} onClick={() => void confirm()}>
                <MailCheck aria-hidden="true" />
                {m.confirm_button()}
              </Button>
            </>
          )}
          {error && (
            <p role="alert" className="m-0 text-destructive text-sm">
              {error}
            </p>
          )}
        </Card>
      </section>
    </Page>
  );
}
