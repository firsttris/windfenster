/** The admin page's alarms and mail: the day's numbers, SMTP with a test mail and the daily
 * limit, push with its key and the browsers by service, every delivery of the last month. */
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import type { AdminAlarms } from '@/bindings/AdminAlarms';
import { Failed } from '@/components/page';
import { date } from '@/lib/admin';
import { adminAlarmsQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { clock, dayMonthOf, formatNumber } from '@/lib/format';
import { describeError } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Chip, ROW, Row, TabHead, Table } from './parts';
import { FIELD, NumberField, SettingRow, useSettings } from './setting';

export function AlarmsTab() {
  const q = useQuery(adminAlarmsQuery);
  if (q.isError) return <Failed error={q.error} />;
  if (!q.data) return <div aria-busy="true" className="h-96 animate-pulse rounded-xl bg-muted" />;
  const a = q.data;
  const devices = a.counts.devices.reduce((n, [, c]) => n + c, 0);
  return (
    <>
      <TabHead
        title={m.admin_tabAlarms()}
        lead={m.admin_alarmsLead({ alarms: a.counts.alarms, spots: a.counts.spots, accounts: a.counts.accounts })}
      />
      <section className="grid grid-cols-[repeat(auto-fit,minmax(170px,1fr))] gap-3">
        <Kpi
          label={m.admin_sentToday()}
          value={a.today.push + a.today.email}
          sub={m.admin_sentTodaySub({ push: a.today.push, email: a.today.email })}
        />
        <Kpi label={m.admin_notDelivered()} value={a.today.failed} sub="" />
        <Kpi label={m.admin_pushDevicesKpi()} value={devices} sub={m.admin_pushDevicesSub()} />
        <Kpi
          label={m.admin_addresses()}
          value={a.counts.confirmed}
          sub={m.admin_addressesSub({ total: a.counts.addresses })}
        />
      </section>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(340px,1fr))] items-start gap-5">
        <SmtpCard data={a} />
        <PushCard data={a} devices={devices} />
      </div>
      <Deliveries data={a} />
    </>
  );
}

function Kpi({ label, value, sub }: { label: string; value: number; sub: string }) {
  return (
    <Card className="gap-1.5 px-[18px] py-4">
      <h2 className="m-0 font-medium text-[13px] text-muted-foreground">{label}</h2>
      <span className="font-bold text-[28px] tracking-[-0.04em]">{formatNumber(value)}</span>
      {sub && <span className="text-muted-foreground text-xs">{sub}</span>}
    </Card>
  );
}

function SmtpCard({ data }: { data: AdminAlarms }) {
  const settings = useSettings();
  const [to, setTo] = useState('');
  const test = useMutation({
    mutationFn: () => send<void>('POST', '/admin/alarms/test-mail', { to }),
  });
  return (
    <Card className="gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_smtp()}</h2>
        <Chip tone={data.smtp ? 'ok' : 'off'}>{data.smtp ? m.admin_smtpOn() : m.admin_smtpOff()}</Chip>
      </div>
      {data.smtp ? (
        <>
          <Row label={m.admin_smtpServer()}>
            <span className="font-mono text-xs">{data.smtp.server}</span>
          </Row>
          <Row label={m.admin_smtpFrom()}>{data.smtp.from}</Row>
          <Row label={m.admin_mailsToday()}>
            {m.admin_mailsTodayValue({ n: formatNumber(data.mailsToday), limit: formatNumber(data.mailPerDay) })}
          </Row>
        </>
      ) : (
        <p className="m-0 text-muted-foreground text-sm">{m.admin_smtpOffText()}</p>
      )}
      {settings.data && (
        <SettingRow
          label={m.admin_mailPerDay()}
          hint={m.admin_mailPerDayHint()}
          lockedBy={settings.locked('mailPerDay')}
        >
          <NumberField
            label={m.admin_mailPerDay()}
            value={settings.data.settings.mailPerDay}
            min={0}
            max={1_000_000}
            disabled={settings.pending}
            onSave={(n) => settings.set('mailPerDay', n)}
          />
        </SettingRow>
      )}
      {data.smtp && (
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            test.mutate();
          }}
        >
          <input
            type="email"
            aria-label={m.admin_testMailTo()}
            placeholder={m.admin_testMailTo()}
            value={to}
            onChange={(e) => setTo(e.target.value)}
            required
            className={cn(FIELD, 'flex-1 font-sans')}
          />
          <Button type="submit" size="sm" disabled={test.isPending}>
            {m.admin_testMail()}
          </Button>
        </form>
      )}
      {test.isSuccess && <p className="m-0 text-muted-foreground text-sm">{m.admin_testMailSent()}</p>}
      {test.isError && <p className="m-0 text-destructive text-sm">{describeError(test.error)}</p>}
      {settings.error && <p className="m-0 text-destructive text-sm">{settings.error}</p>}
    </Card>
  );
}

const SERVICE: Record<string, () => string> = {
  google: m.admin_serviceGoogle,
  mozilla: m.admin_serviceMozilla,
  apple: m.admin_serviceApple,
  microsoft: m.admin_serviceMicrosoft,
  other: m.admin_serviceOther,
};

function PushCard({ data, devices }: { data: AdminAlarms; devices: number }) {
  const top = Math.max(1, ...data.counts.devices.map(([, n]) => n));
  return (
    <Card className="gap-3">
      <h2 className="m-0 font-semibold text-base">{m.admin_push()}</h2>
      <Row label={m.admin_pushKey()}>
        {data.pushKeySince ? m.admin_pushKeySince({ date: date(data.pushKeySince) }) : '–'}
      </Row>
      <div className="flex flex-col gap-1.5 border-line border-t pt-2">
        {data.counts.devices.map(([service, n]) => (
          <div key={service} className="flex items-center gap-2.5 text-[13px]">
            <span className="w-44 shrink-0 text-subtle">{SERVICE[service]?.() ?? service}</span>
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
              <div className="h-full rounded-full bg-foreground" style={{ width: `${(n / top) * 100}%` }} />
            </div>
            <span className="w-8 text-right font-mono text-muted-foreground text-xs">{n}</span>
          </div>
        ))}
        {devices === 0 && <span className="text-muted-foreground text-sm">–</span>}
      </div>
      <p className="m-0 border-line border-t pt-2 text-muted-foreground text-xs leading-relaxed">
        {m.admin_pushKeyText()}
      </p>
    </Card>
  );
}

function Deliveries({ data }: { data: AdminAlarms }) {
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return (
    <Card className="gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="m-0 font-semibold text-base">{m.admin_deliveries2()}</h2>
        <span className="text-muted-foreground text-xs">{m.admin_deliveriesHint()}</span>
      </div>
      {data.deliveries.length === 0 ? (
        <p className="m-0 text-muted-foreground text-sm">{m.admin_noDeliveries()}</p>
      ) : (
        <Table
          cols="0.9fr 1.6fr 1.8fr 0.7fr 1.4fr"
          head={[
            m.admin_colTime(),
            m.admin_colSpotAccount(),
            m.admin_colMessage(),
            m.admin_colChannel(),
            m.admin_colResult(),
          ]}
        >
          {data.deliveries.map((d, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a log, newest first
            <div key={i} className={ROW}>
              <span className="font-mono text-subtle text-xs">
                {dayMonthOf(d.sentAt, zone)} {clock(d.sentAt, zone)}
              </span>
              <span className="flex flex-col">
                <span className="font-medium">{d.kind === 'test' ? m.admin_kindTest() : (d.spot ?? '–')}</span>
                <span className="text-muted-foreground text-xs">{d.account ?? '–'}</span>
              </span>
              <span className="truncate text-subtle" title={d.title}>
                {d.title}
              </span>
              <span className="text-subtle">{d.channel === 'email' ? m.admin_email() : m.admin_push()}</span>
              <span className="min-w-0">
                <Chip tone={d.ok ? 'ok' : 'error'} className="max-w-full truncate">
                  {d.ok ? m.admin_delivered() : (d.detail ?? '–')}
                </Chip>
              </span>
            </div>
          ))}
        </Table>
      )}
    </Card>
  );
}
