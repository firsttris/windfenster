/** One setting of the admin page: shown with its value, changed in place, locked when an
 * environment variable sets it. */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Lock } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import type { AdminSettings } from '@/bindings/AdminSettings';
import type { Settings } from '@/bindings/Settings';
import { adminSettingsQuery } from '@/lib/admin-api';
import { send } from '@/lib/api';
import { describeError } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';

export type SettingKey = keyof Settings;

/** The settings, and a way to change one; `error` says why the last change failed. */
export function useSettings() {
  const queryClient = useQueryClient();
  const query = useQuery(adminSettingsQuery);
  const [error, setError] = useState<string | null>(null);
  const change = useMutation({
    mutationFn: ({ key, value }: { key: SettingKey; value: unknown }) =>
      send<AdminSettings>('PUT', `/admin/settings/${key}`, { value }),
    onSuccess: async (data) => {
      setError(null);
      queryClient.setQueryData(['admin', 'settings'], data);
      await queryClient.invalidateQueries({ queryKey: ['admin'] });
    },
    onError: (e) => setError(describeError(e)),
  });
  return {
    data: query.data,
    error,
    pending: change.isPending,
    set: <K extends SettingKey>(key: K, value: Settings[K]) => change.mutate({ key, value }),
    locked: (key: SettingKey) => query.data?.locked[key] ?? null,
  };
}

export const FIELD =
  'h-9 min-w-0 rounded-[9px] border bg-background px-3 font-mono text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:opacity-60';

/** A label with its hint, the control on the right; a lock where the environment sets it. */
export function SettingRow({
  label,
  hint,
  lockedBy,
  children,
}: {
  label: string;
  hint?: ReactNode;
  lockedBy: string | null;
  children: ReactNode;
}) {
  return (
    <div className="grid grid-cols-[1fr_auto] items-center gap-x-3 gap-y-1 border-line border-b pb-2.5">
      <span className="flex items-center gap-1.5 font-medium text-[13px]">
        {label}
        {lockedBy && (
          <Lock className="size-3 text-muted-foreground" role="img" aria-label={m.admin_locked({ name: lockedBy })} />
        )}
      </span>
      <span className="flex justify-end">{children}</span>
      {(hint || lockedBy) && (
        <span className="col-span-2 text-muted-foreground text-xs">
          {lockedBy ? m.admin_locked({ name: lockedBy }) : hint}
        </span>
      )}
    </div>
  );
}

/** A whole number, saved on Enter or when the field is left. */
export function NumberField({
  label,
  value,
  min,
  max,
  disabled,
  onSave,
  width = 'w-24',
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  disabled: boolean;
  onSave: (n: number) => void;
  width?: string;
}) {
  const [text, setText] = useState(String(value));
  const save = () => {
    const n = Number(text);
    if (Number.isInteger(n) && n >= min && n <= max && n !== value) onSave(n);
    else setText(String(value));
  };
  return (
    <input
      aria-label={label}
      inputMode="numeric"
      value={text}
      disabled={disabled}
      onChange={(e) => setText(e.target.value)}
      onBlur={save}
      onKeyDown={(e) => e.key === 'Enter' && save()}
      className={cn(FIELD, width, 'text-right')}
    />
  );
}

/** A switch for on and off. */
export function Toggle({
  label,
  on,
  disabled,
  onChange,
}: {
  label: string;
  on: boolean;
  disabled: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className={cn(
        'inline-flex h-6 w-10 shrink-0 cursor-pointer items-center rounded-full p-0.5 transition-colors disabled:cursor-not-allowed disabled:opacity-50',
        on ? 'justify-end bg-primary' : 'justify-start bg-secondary',
      )}
    >
      <span className={cn('size-5 rounded-full', on ? 'bg-primary-foreground' : 'bg-dim')} />
    </button>
  );
}

/** A line of text, saved by a button. */
export function TextField({
  label,
  value,
  disabled,
  onSave,
  placeholder,
}: {
  label: string;
  value: string;
  disabled: boolean;
  onSave: (v: string) => void;
  placeholder?: string;
}) {
  const [text, setText] = useState(value);
  return (
    <form
      className="flex gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(text.trim());
      }}
    >
      <input
        aria-label={label}
        value={text}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        className={cn(FIELD, 'flex-1 font-sans')}
      />
      <Button type="submit" size="sm" variant="outline" disabled={disabled || text.trim() === value}>
        {m.admin_save()}
      </Button>
    </form>
  );
}
