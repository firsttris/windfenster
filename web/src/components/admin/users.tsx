/** The admin page's accounts: a searchable list and the picked account with what an admin may do
 * to it. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import type { AdminUser } from '@/bindings/AdminUser';
import { Failed } from '@/components/page';
import { filterUsers, signInKind, type UserFilter } from '@/lib/admin';
import { adminUserQuery, adminUsersQuery } from '@/lib/admin-api';
import { meQuery, send } from '@/lib/api';
import { ago, dayMonthOf } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/ui/dialog';
import { InvitesCard, SignupSwitch } from './invites';
import { Chip, ROW, Row, TabHead, Table } from './parts';
import { useSettings } from './setting';

const FIELD =
  'h-9 min-w-0 rounded-[9px] border bg-background px-3 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60';

export function UsersTab() {
  const users = useQuery(adminUsersQuery);
  const settings = useSettings();
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<UserFilter>('all');
  const [picked, setPicked] = useState<number | null>(null);
  if (users.isError) return <Failed error={users.error} />;
  const now = Date.now() / 1000;
  const list = users.data ? filterUsers(users.data.users, query, filter, now) : [];
  const s = users.data?.summary;
  const selected = picked ?? list[0]?.id ?? null;
  return (
    <>
      <TabHead
        title={m.admin_tabUsers()}
        lead={s && m.admin_usersLead({ total: s.total, week: s.newWeek, month: s.activeMonth })}
      >
        <SignupSwitch />
      </TabHead>
      {settings.data?.settings.signup === 'invite' && <InvitesCard />}
      <div className="flex flex-wrap items-start gap-5">
        <Card className="min-w-0 flex-[999_1_520px] gap-2.5 p-4">
          <div className="flex flex-wrap gap-2">
            <input
              type="search"
              aria-label={m.admin_userSearch()}
              placeholder={m.admin_userSearch()}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className={cn(FIELD, 'flex-[1_1_220px]')}
            />
            {(
              [
                ['all', m.admin_filterAll()],
                ['disabled', m.admin_filterDisabled()],
                ['inactive', m.admin_filterInactive()],
              ] as const
            ).map(([id, label]) => (
              <Button
                key={id}
                size="sm"
                variant={filter === id ? 'outline' : 'ghost'}
                aria-pressed={filter === id}
                onClick={() => setFilter(id)}
              >
                {label}
              </Button>
            ))}
          </div>
          <Table
            cols="2.4fr 1.1fr 0.9fr 0.9fr 0.8fr"
            head={[m.admin_colAccount(), m.admin_colSignIn(), m.admin_colSeen(), m.admin_colSpots(), m.admin_colRole()]}
          >
            {list.map((u) => (
              <UserRow key={u.id} user={u} selected={u.id === selected} onPick={() => setPicked(u.id)} />
            ))}
            {users.data && list.length === 0 && (
              <p className="m-0 p-3 text-muted-foreground text-sm">{m.admin_noUsers()}</p>
            )}
          </Table>
        </Card>
        <div className="flex-[1_1_300px]">
          {selected === null ? (
            <Card>
              <p className="m-0 text-muted-foreground text-sm">{m.admin_pickUser()}</p>
            </Card>
          ) : (
            <UserDetail id={selected} />
          )}
        </div>
      </div>
    </>
  );
}

function signInText(u: AdminUser): string {
  const kind = signInKind(u);
  if (kind === 'both') return m.admin_passkeyAndPassword();
  if (kind === 'passkey') return m.admin_passkey();
  if (kind === 'password') return m.admin_password();
  return m.admin_none();
}

function RoleChip({ user }: { user: AdminUser }) {
  if (user.disabledAt !== null) return <Chip tone="error">{m.admin_rolesDisabled()}</Chip>;
  if (user.admin) return <Chip tone="ok">{m.admin_rolesAdmin()}</Chip>;
  return <Chip tone="off">{m.admin_rolesUser()}</Chip>;
}

function UserRow({ user, selected, onPick }: { user: AdminUser; selected: boolean; onPick: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onPick}
      className={cn(
        ROW,
        'w-full cursor-pointer rounded-[10px] border text-left text-foreground',
        selected ? 'border-border-strong bg-secondary' : 'border-transparent hover:bg-secondary/50',
      )}
    >
      <span className="flex min-w-0 items-center gap-2.5">
        <span className="flex size-[30px] shrink-0 items-center justify-center rounded-full bg-secondary font-semibold text-xs">
          {user.name.charAt(0).toUpperCase()}
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="font-semibold">{user.name}</span>
          <span className="truncate text-muted-foreground text-xs">
            {user.email ?? '–'}
            {user.email && !user.emailConfirmed && ` · ${m.admin_unconfirmed()}`}
          </span>
        </span>
      </span>
      <span className="text-subtle text-xs">{signInText(user)}</span>
      <span className="font-mono text-subtle text-xs">{user.lastSeenAt ? ago(user.lastSeenAt) : m.admin_never()}</span>
      <span className="font-mono text-subtle text-xs">
        {user.favorites} · {user.alarms}
      </span>
      <span>
        <RoleChip user={user} />
      </span>
    </button>
  );
}

function UserDetail({ id }: { id: number }) {
  const queryClient = useQueryClient();
  const me = useQuery(meQuery);
  const detail = useQuery(adminUserQuery(id));
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const act = useMutation({
    mutationFn: ({ method, path, body }: { method: 'POST' | 'PUT' | 'DELETE'; path: string; body?: unknown }) =>
      send<void>(method, `/admin/users/${id}${path}`, body),
    onSuccess: async () => {
      setMessage(m.admin_done());
      await queryClient.invalidateQueries({ queryKey: ['admin'] });
    },
    onError: (e) => setMessage(describeError(e)),
  });
  if (detail.isError) return <Card className="text-sm">{describeError(detail.error)}</Card>;
  if (!detail.data) return <Card aria-busy="true" className="h-80 animate-pulse" />;
  const { user } = detail.data;
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const self = me.data?.id === user.id;
  const run = (method: 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown) => {
    setMessage(null);
    act.mutate({ method, path, body });
  };
  return (
    <Card className="gap-3.5">
      <div className="flex items-center gap-3">
        <span className="flex size-11 items-center justify-center rounded-full bg-secondary font-semibold text-[17px]">
          {user.name.charAt(0).toUpperCase()}
        </span>
        <div className="flex flex-col gap-0.5">
          <span className="font-bold text-lg tracking-[-0.02em]">{user.name}</span>
          <span className="text-muted-foreground text-xs">
            {m.admin_since({ date: dayMonthOf(user.createdAt, zone) })} · {detail.data.locale} · {detail.data.unit}
          </span>
        </div>
      </div>
      <div className="flex flex-col gap-2 border-line border-t pt-3">
        <Row label={m.admin_email()}>
          {user.email ?? '–'}{' '}
          {user.email && (
            <Chip tone={user.emailConfirmed ? 'ok' : 'off'} className="ml-1">
              {user.emailConfirmed ? m.admin_confirmed() : m.admin_unconfirmed()}
            </Chip>
          )}
        </Row>
        <Row label={m.admin_passkeys()}>{user.passkeys}</Row>
        <Row label={m.admin_password()}>{user.password ? '✓' : '–'}</Row>
        <Row label={m.admin_sessions()}>{detail.data.sessions}</Row>
        <Row label={m.admin_pushDevices()}>{detail.data.pushDevices}</Row>
        <Row label={m.admin_userData()}>
          {user.favorites} · {user.alarms} · {detail.data.gear}
        </Row>
        <Row label={m.admin_deliveries()}>{detail.data.deliveries}</Row>
      </div>
      <div className="flex flex-col gap-2 border-line border-t pt-3">
        {user.email && !user.emailConfirmed && (
          <Button variant="outline" disabled={act.isPending} onClick={() => run('POST', '/confirm-email')}>
            {m.admin_resendLink()}
          </Button>
        )}
        <Button variant="outline" disabled={act.isPending} onClick={() => run('POST', '/signout')}>
          {m.admin_signOutEverywhere()}
        </Button>
        <Button variant="outline" asChild>
          <a href={`/api/admin/users/${user.id}/export`} download>
            {m.admin_export()}
          </a>
        </Button>
        <Button variant="outline" disabled={act.isPending} onClick={() => run('PUT', '/role', { admin: !user.admin })}>
          {user.admin ? m.admin_revokeAdmin() : m.admin_makeAdmin()}
        </Button>
        <div className="flex gap-2">
          <Button
            variant="outline"
            className="flex-1 border-warning-border text-warning-foreground"
            disabled={act.isPending || self}
            onClick={() => run('POST', user.disabledAt === null ? '/disable' : '/enable')}
          >
            {user.disabledAt === null ? m.admin_disable() : m.admin_enable()}
          </Button>
          <Button
            variant="outline"
            className="flex-1 border-destructive/40 text-destructive"
            disabled={act.isPending || self}
            onClick={() => setConfirmDelete(true)}
          >
            {m.admin_delete()}
          </Button>
        </div>
        {message && (
          <p role="status" className="m-0 text-muted-foreground text-sm">
            {message}
          </p>
        )}
      </div>
      <p className="m-0 text-muted-foreground text-xs leading-relaxed">{m.admin_passwordNote()}</p>
      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent className="flex flex-col gap-4 p-5">
          <DialogTitle className="m-0 font-semibold text-lg">{m.admin_deleteTitle({ name: user.name })}</DialogTitle>
          <DialogDescription className="m-0 text-muted-foreground text-sm leading-relaxed">
            {m.admin_deleteText()}
          </DialogDescription>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setConfirmDelete(false)}>
              {m.admin_cancel()}
            </Button>
            <Button
              className="bg-destructive text-white hover:bg-destructive/90"
              onClick={() => {
                setConfirmDelete(false);
                run('DELETE', '');
              }}
            >
              {m.admin_delete()}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
