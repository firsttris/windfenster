/** Push messages on this device: on, off, refused, or how to get them (an iPhone needs the site
 * on its home screen first). */
import { useQuery } from '@tanstack/react-query';
import { BellOff, BellRing } from 'lucide-react';
import { useEffect, useState } from 'react';
import { pushInfoQuery } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { currentPush, disablePush, enablePush, pushSupport } from '@/lib/push';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';

type State = 'loading' | 'on' | 'off' | 'denied';

export function usePushState(): [State, () => void] {
  const [state, setState] = useState<State>('loading');
  const [round, setRound] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `round` asks again after a change
  useEffect(() => {
    if (pushSupport() !== 'ok') return setState('off');
    void currentPush().then((sub) => setState(sub ? 'on' : Notification.permission === 'denied' ? 'denied' : 'off'));
  }, [round]);
  return [state, () => setRound((r) => r + 1)];
}

export function PushDevice() {
  const info = useQuery(pushInfoQuery);
  const [state, refresh] = usePushState();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const support = pushSupport();
  const key = info.data?.key ?? null;

  const toggle = async () => {
    setBusy(true);
    setError(null);
    try {
      if (state === 'on') await disablePush();
      else if (key && !(await enablePush(key))) setError(m.push_refused());
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(false);
      refresh();
    }
  };

  if (support === 'home-screen') return <p className="m-0 text-muted-foreground text-sm">{m.push_homeScreen()}</p>;
  if (support === 'unsupported' || (info.data && !key))
    return <p className="m-0 text-muted-foreground text-sm">{m.push_unsupported()}</p>;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="flex items-center gap-2 text-sm">
          {state === 'on' ? (
            <BellRing aria-hidden="true" className="size-4 text-emerald-500" />
          ) : (
            <BellOff aria-hidden="true" className="size-4 text-muted-foreground" />
          )}
          {state === 'on' ? m.push_on() : state === 'denied' ? m.push_denied() : m.push_off()}
        </span>
        {state !== 'denied' && state !== 'loading' && (
          <Button type="button" variant="outline" size="sm" disabled={busy || !key} onClick={() => void toggle()}>
            {state === 'on' ? m.push_disable() : m.push_enable()}
          </Button>
        )}
      </div>
      {error && (
        <p role="alert" className="m-0 text-destructive text-sm">
          {error}
        </p>
      )}
    </div>
  );
}
