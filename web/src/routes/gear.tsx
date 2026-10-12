import { createFileRoute } from '@tanstack/react-router';
import { Plus, Trash2 } from 'lucide-react';
import { type FormEvent, useEffect, useState } from 'react';
import type { GearItem } from '@/bindings/GearItem';
import type { GearKind } from '@/bindings/GearKind';
import { KIND_NAME } from '@/components/gear-fit';
import { Page } from '@/components/page';
import { GEAR_KINDS, type GearLevel, template } from '@/lib/gear';
import { describeError } from '@/lib/i18n';
import { useGear } from '@/lib/use-gear';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';
import { Card } from '@/ui/card';
import { Segmented } from '@/ui/segmented';

export const Route = createFileRoute('/gear')({
  component: GearPage,
});

const INPUT =
  'h-10 w-full min-w-0 rounded-md border bg-background px-2 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring/60';

/** A row as typed: numbers as text until saved */
interface Draft {
  kind: GearKind;
  size: string;
  minKn: string;
  maxKn: string;
}

const draft = (g: GearItem): Draft => ({
  kind: g.kind,
  size: String(g.size),
  minKn: String(g.minKn),
  maxKn: String(g.maxKn),
});
const num = (s: string) => Number(s.replace(',', '.'));
const valid = (d: Draft) =>
  num(d.size) > 0 && num(d.size) <= 50 && num(d.minKn) >= 0 && num(d.maxKn) <= 80 && num(d.minKn) < num(d.maxKn);

/** The user's kites, wings, sails and parawings with the mean wind each works in. */
function GearPage() {
  useTitle(m.title_gear());
  const { gear, save, ready } = useGear();
  const [rows, setRows] = useState<Draft[]>([]);
  const [kind, setKind] = useState<GearKind>('wing');
  const [level, setLevel] = useState<GearLevel>('intermediate');
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: filled once the saved list is there
  useEffect(() => {
    if (ready) setRows(gear.map(draft));
  }, [ready]);

  const set = (i: number, patch: Partial<Draft>) => setRows((r) => r.map((d, k) => (k === i ? { ...d, ...patch } : d)));
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setMessage(null);
    try {
      await save(rows.map((d) => ({ kind: d.kind, size: num(d.size), minKn: num(d.minKn), maxKn: num(d.maxKn) })));
      setMessage({ error: false, text: m.gear_saved() });
    } catch (err) {
      setMessage({ error: true, text: describeError(err) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Page>
      <section className="mx-auto flex w-full max-w-2xl flex-col gap-5 py-6">
        <div className="flex flex-col gap-2">
          <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.gear_title()}</h1>
          <p className="m-0 text-subtle leading-relaxed">{m.gear_lead()}</p>
        </div>
        <Card>
          <form onSubmit={submit} className="flex flex-col gap-3">
            {rows.length > 0 && (
              <div className="grid grid-cols-[1.4fr_1fr_1fr_1fr_auto] items-end gap-2 text-muted-foreground text-xs">
                <span>{m.gear_kind()}</span>
                <span>{m.gear_size()}</span>
                <span>{m.gear_from()}</span>
                <span>{m.gear_to()}</span>
                <span className="w-10" />
              </div>
            )}
            {rows.map((d, i) => (
              <div
                // biome-ignore lint/suspicious/noArrayIndexKey: rows have no id until saved
                key={i}
                className="grid grid-cols-[1.4fr_1fr_1fr_1fr_auto] items-center gap-2"
              >
                <select
                  aria-label={m.gear_kind()}
                  value={d.kind}
                  onChange={(e) => set(i, { kind: e.target.value as GearKind })}
                  className={INPUT}
                >
                  {GEAR_KINDS.map((k) => (
                    <option key={k} value={k}>
                      {KIND_NAME[k]()}
                    </option>
                  ))}
                </select>
                <input
                  aria-label={m.gear_size()}
                  inputMode="decimal"
                  value={d.size}
                  onChange={(e) => set(i, { size: e.target.value })}
                  className={INPUT}
                />
                <input
                  aria-label={m.gear_from()}
                  inputMode="decimal"
                  value={d.minKn}
                  onChange={(e) => set(i, { minKn: e.target.value })}
                  className={INPUT}
                />
                <input
                  aria-label={m.gear_to()}
                  inputMode="decimal"
                  value={d.maxKn}
                  onChange={(e) => set(i, { maxKn: e.target.value })}
                  className={INPUT}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={m.gear_remove()}
                  onClick={() => setRows((r) => r.filter((_, k) => k !== i))}
                >
                  <Trash2 aria-hidden="true" />
                </Button>
              </div>
            ))}
            {rows.length === 0 && ready && <p className="m-0 text-muted-foreground text-sm">{m.gear_empty()}</p>}
            <div className="flex flex-wrap justify-between gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() =>
                  setRows((r) => [...r, { kind: r.at(-1)?.kind ?? 'wing', size: '', minKn: '', maxKn: '' }])
                }
              >
                <Plus aria-hidden="true" />
                {m.gear_add()}
              </Button>
              <Button type="submit" disabled={busy || !rows.every(valid)}>
                {m.gear_save()}
              </Button>
            </div>
            {message && (
              <p
                role={message.error ? 'alert' : undefined}
                className={message.error ? 'm-0 text-destructive text-sm' : 'm-0 text-muted-foreground text-sm'}
              >
                {message.text}
              </p>
            )}
          </form>
        </Card>
        <Card>
          <h2 className="m-0 font-medium text-[13px] text-muted-foreground">{m.gear_template()}</h2>
          <p className="m-0 text-muted-foreground text-sm leading-relaxed">{m.gear_templateHint()}</p>
          <div className="flex flex-wrap items-center gap-2">
            <select
              aria-label={m.gear_kind()}
              value={kind}
              onChange={(e) => setKind(e.target.value as GearKind)}
              className={INPUT.replace('w-full ', 'w-auto ')}
            >
              {GEAR_KINDS.map((k) => (
                <option key={k} value={k}>
                  {KIND_NAME[k]()}
                </option>
              ))}
            </select>
            <Segmented
              label={m.gear_level()}
              value={level}
              onChange={setLevel}
              options={[
                { value: 'beginner', label: m.gear_beginner() },
                { value: 'intermediate', label: m.gear_intermediate() },
                { value: 'expert', label: m.gear_expert() },
              ]}
            />
          </div>
          <Button
            variant="outline"
            className="w-fit"
            onClick={() => setRows((r) => [...r.filter((d) => d.kind !== kind), ...template(kind, level).map(draft)])}
          >
            {m.gear_useTemplate()}
          </Button>
        </Card>
      </section>
    </Page>
  );
}
