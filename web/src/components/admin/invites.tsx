/** Sign-up on the admin page: open, by invitation or closed; the invitations with their links. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import type { Invite } from '@/bindings/Invite';
import type { NewInvite } from '@/bindings/NewInvite';
import type { SignupMode } from '@/bindings/SignupMode';
import { date } from '@/lib/admin';
import { adminInvitesQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Segmented } from '@/ui/segmented';
import { Chip } from './parts';
import { useSettings } from './setting';

export function SignupSwitch() {
  const settings = useSettings();
  const value = settings.data?.settings.signup ?? null;
  return (
    <div className="flex flex-col items-end gap-1.5">
      <span className="text-muted-foreground text-xs">{m.admin_signup()}</span>
      <Segmented<SignupMode>
        label={m.admin_signup()}
        value={value}
        options={[
          { value: 'open', label: m.admin_signupOpen() },
          { value: 'invite', label: m.admin_signupInvite() },
          { value: 'closed', label: m.admin_signupClosed() },
        ]}
        onChange={(v) => settings.set('signup', v)}
      />
    </div>
  );
}

/** A link relative to this site made whole, for sending it on. */
function whole(link: string): string {
  return link.startsWith('/') ? `${window.location.origin}${link}` : link;
}

function state(i: Invite, now: number): { tone: 'ok' | 'off' | 'warn'; text: string } {
  if (i.usedAt !== null) return { tone: 'ok', text: m.admin_inviteUsed({ name: i.usedBy ?? '–' }) };
  if (i.expiresAt <= now) return { tone: 'off', text: m.admin_inviteExpired() };
  return { tone: 'warn', text: m.admin_inviteOpen({ date: date(i.expiresAt) }) };
}

export function InvitesCard() {
  const queryClient = useQueryClient();
  const invites = useQuery(adminInvitesQuery);
  const [made, setMade] = useState<NewInvite | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () => send<NewInvite>('POST', '/admin/invites'),
    onSuccess: async (n) => {
      setMade(n);
      setCopied(false);
      await queryClient.invalidateQueries({ queryKey: ['admin', 'invites'] });
    },
    onError: (e) => setError(describeError(e)),
  });
  const withdraw = useMutation({
    mutationFn: (id: string) => send<void>('DELETE', `/admin/invites/${id}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['admin', 'invites'] }),
    onError: (e) => setError(describeError(e)),
  });
  const now = Date.now() / 1000;
  return (
    <Card className="gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_invites()}</h2>
        <Button size="sm" disabled={create.isPending} onClick={() => create.mutate()}>
          {m.admin_inviteCreate()}
        </Button>
      </div>
      <p className="m-0 text-muted-foreground text-[13px]">{m.admin_invitesText()}</p>
      {made && (
        <div className="flex flex-col gap-2 rounded-xl border bg-background p-3">
          <span className="text-muted-foreground text-xs">{m.admin_inviteLink()}</span>
          <div className="flex flex-wrap items-center gap-2">
            <code className="min-w-0 flex-1 break-all font-mono text-xs">{whole(made.link)}</code>
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                void navigator.clipboard?.writeText(whole(made.link));
                setCopied(true);
              }}
            >
              {copied ? m.admin_inviteCopied() : m.admin_inviteCopy()}
            </Button>
          </div>
        </div>
      )}
      {invites.data?.length === 0 && <p className="m-0 text-muted-foreground text-sm">{m.admin_noInvites()}</p>}
      {invites.data?.map((i) => {
        const st = state(i, now);
        return (
          <div
            key={i.id}
            className="flex flex-wrap items-center justify-between gap-2 border-line border-t pt-2.5 text-[13px]"
          >
            <span className="flex flex-col">
              <span className="font-mono text-xs">{date(i.createdAt)}</span>
              {i.createdBy && (
                <span className="text-muted-foreground text-xs">{m.admin_inviteBy({ name: i.createdBy })}</span>
              )}
            </span>
            <span className="flex items-center gap-2">
              <Chip tone={st.tone}>{st.text}</Chip>
              {i.usedAt === null && i.expiresAt > now && (
                <Button size="sm" variant="outline" disabled={withdraw.isPending} onClick={() => withdraw.mutate(i.id)}>
                  {m.admin_inviteWithdraw()}
                </Button>
              )}
            </span>
          </div>
        );
      })}
      {error && <p className="m-0 text-destructive text-sm">{error}</p>}
    </Card>
  );
}
