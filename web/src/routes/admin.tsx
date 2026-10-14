import { useQuery } from '@tanstack/react-query';
import { createFileRoute, Link, Navigate } from '@tanstack/react-router';
import { LayoutDashboard, Users } from 'lucide-react';
import type { ComponentType, ReactNode } from 'react';
import { OverviewTab } from '@/components/admin/overview';
import { UsersTab } from '@/components/admin/users';
import { Loading, Page } from '@/components/page';
import { type AdminTab, parseTab } from '@/lib/admin';
import { meQuery } from '@/lib/api';
import { useTitle } from '@/lib/use-title';
import { cn } from '@/lib/utils';
import { m } from '@/paraglide/messages';

/** The tabs built so far, in the order of the page's menu. */
const TABS: {
  id: AdminTab;
  label: () => string;
  icon: ComponentType<{ className?: string }>;
  view: () => ReactNode;
}[] = [
  { id: 'overview', label: m.admin_tabOverview, icon: LayoutDashboard, view: () => <OverviewTab /> },
  { id: 'users', label: m.admin_tabUsers, icon: Users, view: () => <UsersTab /> },
];

interface AdminSearch {
  tab?: AdminTab;
}

export const Route = createFileRoute('/admin')({
  validateSearch: (s: Record<string, unknown>): AdminSearch => ({
    tab:
      s.tab === undefined
        ? undefined
        : parseTab(
            s.tab,
            TABS.map((t) => t.id),
          ),
  }),
  component: AdminPage,
});

function AdminPage() {
  useTitle(m.title_admin());
  const me = useQuery(meQuery);
  const { tab } = Route.useSearch();
  if (me.isPending) return <Loading />;
  if (!me.data) return <Navigate to="/login" search={{ redirect: '/admin' }} />;
  if (!me.data.admin) {
    return (
      <Page>
        <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{m.title_admin()}</h1>
        <p className="m-0 max-w-xl text-muted-foreground">{m.admin_onlyAdmins()}</p>
      </Page>
    );
  }
  const current = TABS.find((t) => t.id === tab) ?? TABS[0];
  return (
    <Page>
      <div className="flex flex-wrap items-start gap-7">
        <aside className="flex max-w-[260px] flex-[1_1_220px] flex-col gap-1 max-sm:max-w-none">
          <div className="px-3 pb-2 font-medium text-[11px] text-muted-foreground uppercase tracking-[0.1em]">
            {m.nav_admin()}
          </div>
          <nav aria-label={m.admin_sections()} className="flex flex-col gap-0.5 max-sm:flex-row max-sm:overflow-x-auto">
            {TABS.map((t) => (
              <Link
                key={t.id}
                to="/admin"
                search={{ tab: t.id }}
                aria-current={t.id === current.id ? 'page' : undefined}
                className={cn(
                  'flex min-h-11 items-center gap-2.5 whitespace-nowrap rounded-[10px] border px-3 font-medium text-sm no-underline',
                  t.id === current.id
                    ? 'border-border bg-secondary text-foreground'
                    : 'border-transparent text-muted-foreground hover:text-foreground',
                )}
              >
                <t.icon className="size-[18px] shrink-0" />
                {t.label()}
              </Link>
            ))}
          </nav>
        </aside>
        <div className="flex min-w-0 flex-[999_1_640px] flex-col gap-5">{current.view()}</div>
      </div>
    </Page>
  );
}
