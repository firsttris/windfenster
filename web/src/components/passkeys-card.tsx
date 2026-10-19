/** The account's passkeys: one per device or password manager, with when each was added and
 * last used; another one is added on this device, one no longer wanted removed (not the last
 * way into the account). */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { KeyRound, Trash2 } from 'lucide-react';
import { useState } from 'react';
import type { Account } from '@/bindings/Account';
import type { PasskeySummary } from '@/bindings/PasskeySummary';
import { passkeysQuery, send } from '@/lib/api';
import { ago, clock, dateOf } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { addPasskey, cancelled } from '@/lib/passkeys';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card, CardHeader, CardTitle } from '@/ui/card';

export function PasskeysCard({ account }: { account: Account }) {
  const queryClient = useQueryClient();
  const list = useQuery(passkeysQuery);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  // The server decides; this only spares a button that would be refused.
  const onlyWayIn = account.passkeys <= 1 && !account.email;

  const run = async (action: () => Promise<Account>) => {
    setBusy(true);
    setError(null);
    try {
      queryClient.setQueryData(['me'], await action());
      setConfirming(null);
    } catch (e) {
      if (!cancelled(e)) setError(describeError(e));
    } finally {
      setBusy(false);
      await queryClient.invalidateQueries({ queryKey: ['passkeys'] });
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{m.account_passkeys()}</CardTitle>
      </CardHeader>
      <p className="m-0 text-sm text-subtle">{m.account_passkeysCount({ count: account.passkeys })}</p>
      {list.data && list.data.length > 0 && (
        <ul className="m-0 flex list-none flex-col divide-y rounded-md border p-0">
          {list.data.map((k) => (
            <Row
              key={k.id}
              passkey={k}
              busy={busy}
              removable={!onlyWayIn}
              confirming={confirming === k.id}
              onAsk={() => setConfirming(k.id)}
              onRemove={() => void run(() => send<Account>('DELETE', `/me/passkeys/${k.id}`))}
            />
          ))}
        </ul>
      )}
      {confirming && (
        <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_removePasskeyHint()}</p>
      )}
      {onlyWayIn && list.data?.length === 1 && (
        <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_lastPasskey()}</p>
      )}
      <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_passkeysHint()}</p>
      <Button variant="outline" className="w-fit" disabled={busy} onClick={() => void run(addPasskey)}>
        <KeyRound aria-hidden="true" />
        {m.account_addPasskey()}
      </Button>
      {error && (
        <p role="alert" className="m-0 text-destructive text-sm">
          {error}
        </p>
      )}
    </Card>
  );
}

function Row({
  passkey,
  busy,
  removable,
  confirming,
  onAsk,
  onRemove,
}: {
  passkey: PasskeySummary;
  busy: boolean;
  removable: boolean;
  confirming: boolean;
  onAsk: () => void;
  onRemove: () => void;
}) {
  // The viewer's own time zone; the time tells two passkeys of one day apart.
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const when = { date: dateOf(passkey.createdAt, zone), time: clock(passkey.createdAt, zone) };
  return (
    <li className="flex items-center gap-3 px-3 py-2.5">
      <KeyRound aria-hidden="true" className="size-4 shrink-0 text-subtle" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="font-medium text-sm">{m.account_passkeyAdded(when)}</span>
        <span className="text-muted-foreground text-xs">
          {passkey.lastUsedAt ? m.account_passkeyUsed({ ago: ago(passkey.lastUsedAt) }) : m.account_passkeyUnused()}
        </span>
      </div>
      {removable &&
        (confirming ? (
          <Button
            variant="outline"
            size="sm"
            className="border-destructive/50 text-destructive"
            disabled={busy}
            onClick={onRemove}
          >
            <Trash2 aria-hidden="true" />
            {m.account_removePasskeyConfirm()}
          </Button>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            aria-label={m.account_removePasskey(when)}
            title={m.account_removePasskey(when)}
            disabled={busy}
            onClick={onAsk}
          >
            <Trash2 aria-hidden="true" />
          </Button>
        ))}
    </li>
  );
}
