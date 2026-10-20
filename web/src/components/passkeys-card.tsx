/** The account's passkeys: one per device or password manager, named after the browser and
 * system it was added with or by the user, with when it was added and last used; another one is
 * added on this device, one no longer wanted removed (not the last way into the account). */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, KeyRound, Pencil, Trash2, X } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import type { Account } from '@/bindings/Account';
import type { PasskeySummary } from '@/bindings/PasskeySummary';
import { passkeysQuery, send } from '@/lib/api';
import { ago, clock, dateOf } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { passkeyLabel } from '@/lib/passkey-label';
import { addPasskey, cancelled } from '@/lib/passkeys';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card, CardHeader, CardTitle } from '@/ui/card';

/** What a row is doing: nothing, asking to confirm the removal, or taking a new name */
type Mode = { id: string; kind: 'remove' | 'rename' } | null;

export function PasskeysCard({ account }: { account: Account }) {
  const queryClient = useQueryClient();
  const list = useQuery(passkeysQuery);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>(null);
  // The server decides; this only spares a button that would be refused.
  const onlyWayIn = account.passkeys <= 1 && !account.email;

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      setMode(null);
    } catch (e) {
      if (!cancelled(e)) setError(describeError(e));
    } finally {
      setBusy(false);
      await queryClient.invalidateQueries({ queryKey: ['passkeys'] });
    }
  };
  const setMe = async (a: Promise<Account>) => {
    queryClient.setQueryData(['me'], await a);
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
              mode={mode?.id === k.id ? mode.kind : null}
              onMode={(kind) => setMode(kind && { id: k.id, kind })}
              onRemove={() => void run(() => setMe(send<Account>('DELETE', `/me/passkeys/${k.id}`)))}
              onRename={(name) => void run(() => send('PATCH', `/me/passkeys/${k.id}`, { name }))}
            />
          ))}
        </ul>
      )}
      {mode?.kind === 'remove' && (
        <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_removePasskeyHint()}</p>
      )}
      {onlyWayIn && list.data?.length === 1 && (
        <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_lastPasskey()}</p>
      )}
      <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.account_passkeysHint()}</p>
      <Button variant="outline" className="w-fit" disabled={busy} onClick={() => void run(() => setMe(addPasskey()))}>
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
  mode,
  onMode,
  onRemove,
  onRename,
}: {
  passkey: PasskeySummary;
  busy: boolean;
  removable: boolean;
  mode: 'remove' | 'rename' | null;
  onMode: (kind: 'remove' | 'rename' | null) => void;
  onRemove: () => void;
  onRename: (name: string) => void;
}) {
  // The viewer's own time zone; the time tells two passkeys of one day apart.
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const added = m.account_passkeyAdded({ date: dateOf(passkey.createdAt, zone), time: clock(passkey.createdAt, zone) });
  const used = passkey.lastUsedAt ? m.account_passkeyUsed({ ago: ago(passkey.lastUsedAt) }) : m.account_passkeyUnused();
  const name = passkeyLabel(passkey);

  if (mode === 'rename')
    return <RenameRow passkey={passkey} busy={busy} onCancel={() => onMode(null)} onSave={onRename} />;
  return (
    <li className="flex items-center gap-3 px-3 py-2.5">
      <KeyRound aria-hidden="true" className="size-4 shrink-0 text-subtle" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-medium text-sm">{name}</span>
        <span className="text-muted-foreground text-xs">
          {added} · {used}
        </span>
      </div>
      {mode === 'remove' ? (
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
        <div className="flex shrink-0">
          <Button
            variant="ghost"
            size="sm"
            aria-label={m.account_renamePasskey({ name })}
            title={m.account_renamePasskey({ name })}
            disabled={busy}
            onClick={() => onMode('rename')}
          >
            <Pencil aria-hidden="true" />
          </Button>
          {removable && (
            <Button
              variant="ghost"
              size="sm"
              aria-label={m.account_removePasskey({ name })}
              title={m.account_removePasskey({ name })}
              disabled={busy}
              onClick={() => onMode('remove')}
            >
              <Trash2 aria-hidden="true" />
            </Button>
          )}
        </div>
      )}
    </li>
  );
}

/** The row taking the user's own name; empty goes back to the browser and system. */
function RenameRow({
  passkey,
  busy,
  onCancel,
  onSave,
}: {
  passkey: PasskeySummary;
  busy: boolean;
  onCancel: () => void;
  onSave: (name: string) => void;
}) {
  const [name, setName] = useState(passkey.name ?? '');
  const save = (e: FormEvent) => {
    e.preventDefault();
    onSave(name);
  };
  return (
    <li className="px-3 py-2.5">
      <form onSubmit={save} className="flex items-center gap-2">
        <input
          aria-label={m.account_passkeyName()}
          placeholder={passkeyLabel({ ...passkey, name: null })}
          title={m.account_passkeyNameHint()}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Escape' && onCancel()}
          maxLength={60}
          // biome-ignore lint/a11y/noAutofocus: opened by the user's click on the pencil
          autoFocus
          className="h-8 min-w-0 flex-1 rounded-sm border bg-background px-2.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        />
        <Button type="submit" variant="outline" size="sm" aria-label={m.account_save()} disabled={busy}>
          <Check aria-hidden="true" />
        </Button>
        <Button type="button" variant="ghost" size="sm" aria-label={m.account_cancel()} onClick={onCancel}>
          <X aria-hidden="true" />
        </Button>
      </form>
    </li>
  );
}
