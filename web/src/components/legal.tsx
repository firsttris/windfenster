/** The operator's legal notice or privacy policy, as written on the admin page: plain text,
 * paragraphs separated by a blank line. */
import { useQuery } from '@tanstack/react-query';
import { Failed, Loading, Page } from '@/components/page';
import { api, isApiError } from '@/lib/api';
import { useTitle } from '@/lib/use-title';
import { m } from '@/paraglide/messages';

export function LegalPage({ page, title }: { page: 'imprint' | 'privacy'; title: string }) {
  useTitle(title);
  const text = useQuery({ queryKey: ['site', page], queryFn: () => api<string>(`/site/${page}`) });
  if (text.isPending) return <Loading />;
  if (text.isError && !(isApiError(text.error) && text.error.status === 404)) return <Failed error={text.error} />;
  const paragraphs = (text.data ?? '').split(/\n\s*\n/).filter((p) => p.trim());
  return (
    <Page>
      <article className="mx-auto flex w-full max-w-2xl flex-col gap-4 py-6">
        <h1 className="m-0 font-bold text-4xl tracking-[-0.05em]">{title}</h1>
        {paragraphs.length === 0 && <p className="m-0 text-muted-foreground">{m.legal_none()}</p>}
        {paragraphs.map((p, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed text
          <p key={i} className="m-0 whitespace-pre-line text-subtle leading-relaxed">
            {p.trim()}
          </p>
        ))}
      </article>
    </Page>
  );
}
