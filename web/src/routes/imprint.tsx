import { createFileRoute } from '@tanstack/react-router';
import { LegalPage } from '@/components/legal';
import { m } from '@/paraglide/messages';

export const Route = createFileRoute('/imprint')({
  component: () => <LegalPage page="imprint" title={m.title_imprint()} />,
});
