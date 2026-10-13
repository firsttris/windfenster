/** A spot's alarm: a bell (lit while one is set) that opens its settings: from how much wind,
 * only when the gear fits, ahead from the forecast and/or live from the station, by push and/or
 * e-mail. The server checks every ten minutes and stays quiet at night. */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Bell, BellRing } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import type { Alarm } from '@/bindings/Alarm';
import type { AlarmSettings } from '@/bindings/AlarmSettings';
import type { Favorite } from '@/bindings/Favorite';
import { clientPrefs, newAlarm } from '@/lib/alarm';
import { alarmsQuery, meQuery, pushInfoQuery, send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { useUnit } from '@/lib/prefs';
import { KMH_PER_KN, MS_PER_KN, UNIT_LABEL } from '@/lib/units';
import { useGear } from '@/lib/use-gear';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/ui/dialog';
import { PushDevice } from './push-device';

const PER_KN = { kn: 1, ms: MS_PER_KN, kmh: KMH_PER_KN } as const;

/** The bell; `compact` as an icon only, for the spot cards. */
export function AlarmButton({ favorite, compact = false }: { favorite: Favorite; compact?: boolean }) {
  const alarms = useQuery(alarmsQuery);
  const [open, setOpen] = useState(false);
  const alarm = alarms.data?.find((a) => a.favoriteId === favorite.id) ?? null;
  const Icon = alarm ? BellRing : Bell;
  const label = alarm ? m.alarm_on() : m.alarm_add();
  return (
    <>
      {compact ? (
        <button
          type="button"
          aria-label={`${label}: ${favorite.name}`}
          aria-pressed={Boolean(alarm)}
          onClick={() => setOpen(true)}
          className={cn(
            'relative z-10 inline-flex size-8 cursor-pointer items-center justify-center rounded-md outline-none hover:bg-secondary focus-visible:ring-2 focus-visible:ring-ring/60',
            alarm ? 'text-amber-500' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Icon aria-hidden="true" className="size-4" />
        </button>
      ) : (
        <Button variant={alarm ? 'secondary' : 'outline'} aria-pressed={Boolean(alarm)} onClick={() => setOpen(true)}>
          <Icon aria-hidden="true" className={alarm ? 'text-amber-500' : undefined} />
          {label}
        </Button>
      )}
      {open && <AlarmDialog favorite={favorite} alarm={alarm} onClose={() => setOpen(false)} />}
    </>
  );
}

function AlarmDialog({ favorite, alarm, onClose }: { favorite: Favorite; alarm: Alarm | null; onClose: () => void }) {
  const queryClient = useQueryClient();
  const me = useQuery(meQuery);
  const info = useQuery(pushInfoQuery);
  const { gear } = useGear();
  const [unit] = useUnit();
  const station = favorite.slug !== null;
  const [s, setS] = useState<AlarmSettings>(() => alarm?.settings ?? newAlarm(gear, station));
  const [wind, setWind] = useState(() => round(s.minKn * PER_KN[unit]));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<AlarmSettings>) => setS((x) => ({ ...x, ...patch }));
  // E-mails go only to a confirmed address.
  const mailReady = Boolean(info.data?.email && me.data?.email && me.data.emailConfirmed);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      onClose();
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(false);
    }
  };
  const save = (e: FormEvent) => {
    e.preventDefault();
    const settings: AlarmSettings = {
      ...s,
      minKn: Number(wind.replace(',', '.')) / PER_KN[unit],
      gear: s.gear && gear.length > 0,
      live: s.live && station,
      email: s.email && mailReady,
    };
    void run(async () => {
      const saved = await send<Alarm>('PUT', `/alarms/${favorite.id}`, { settings, client: clientPrefs(unit) });
      queryClient.setQueryData<Alarm[]>(['alarms'], (l) => [
        ...(l ?? []).filter((a) => a.favoriteId !== saved.favoriteId),
        saved,
      ]);
    });
  };
  const remove = () =>
    run(async () => {
      await send('DELETE', `/alarms/${favorite.id}`);
      queryClient.setQueryData<Alarm[]>(['alarms'], (l) => l?.filter((a) => a.favoriteId !== favorite.id));
    });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="p-5">
        <form onSubmit={save} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1">
            <DialogTitle className="m-0 font-bold text-xl tracking-[-0.03em]">
              {m.alarm_title({ name: favorite.name })}
            </DialogTitle>
            <DialogDescription className="m-0 text-muted-foreground text-sm">{m.alarm_lead()}</DialogDescription>
          </div>
          <label className="flex items-center justify-between gap-3 font-medium text-sm">
            {m.alarm_from()}
            <span className="flex items-center gap-2 font-normal">
              <input
                inputMode="decimal"
                value={wind}
                onChange={(e) => setWind(e.target.value)}
                className="h-10 w-20 rounded-md border bg-background px-3 text-right text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              />
              <span className="w-10 text-muted-foreground">{UNIT_LABEL[unit]}</span>
            </span>
          </label>
          {gear.length > 0 && (
            <Check
              checked={s.gear}
              onChange={(gear) => set({ gear })}
              label={m.alarm_gear()}
              hint={m.alarm_gearHint()}
            />
          )}
          <fieldset className="m-0 flex flex-col gap-2.5 border-0 p-0">
            <legend className="mb-1.5 p-0 font-medium text-sm">{m.alarm_when()}</legend>
            <Check
              checked={s.forecast}
              onChange={(forecast) => set({ forecast })}
              label={m.alarm_forecast()}
              hint={m.alarm_forecastHint()}
            />
            {station && (
              <Check
                checked={s.live}
                onChange={(live) => set({ live })}
                label={m.alarm_live()}
                hint={m.alarm_liveHint()}
              />
            )}
          </fieldset>
          <fieldset className="m-0 flex flex-col gap-2.5 border-0 p-0">
            <legend className="mb-1.5 p-0 font-medium text-sm">{m.alarm_how()}</legend>
            <Check checked={s.push} onChange={(push) => set({ push })} label={m.alarm_push()} />
            {s.push && (
              <div className="ml-6 rounded-md border bg-muted/40 px-3 py-2.5">
                <PushDevice />
              </div>
            )}
            <Check
              checked={s.email && mailReady}
              disabled={!mailReady}
              onChange={(email) => set({ email })}
              label={
                !me.data?.email
                  ? m.alarm_emailNone()
                  : me.data.emailConfirmed
                    ? m.alarm_email({ email: me.data.email })
                    : m.alarm_emailUnconfirmed()
              }
              hint={info.data && !info.data.email ? m.alarm_emailOff() : undefined}
            />
          </fieldset>
          <p className="m-0 text-muted-foreground text-xs">{m.alarm_night()}</p>
          {error && (
            <p role="alert" className="m-0 text-destructive text-sm">
              {error}
            </p>
          )}
          <div className="flex flex-wrap justify-between gap-2">
            {alarm ? (
              <Button type="button" variant="ghost" disabled={busy} onClick={() => void remove()}>
                {m.alarm_remove()}
              </Button>
            ) : (
              <span />
            )}
            <Button type="submit" disabled={busy}>
              {m.alarm_save()}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function Check({
  checked,
  onChange,
  label,
  hint,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <label className={cn('flex items-start gap-2 text-sm', disabled && 'text-muted-foreground')}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 shrink-0"
      />
      <span className="flex flex-col gap-0.5">
        {label}
        {hint && <span className="text-muted-foreground text-xs">{hint}</span>}
      </span>
    </label>
  );
}

/** A wind as typed: whole knots and km/h, m/s to one place */
function round(v: number): string {
  return String(Math.round(v * 10) / 10);
}
