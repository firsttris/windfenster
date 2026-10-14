import { createFileRoute } from '@tanstack/react-router';
import { LegalPage } from '@/components/legal';
import { m } from '@/paraglide/messages';

export const Route = createFileRoute('/privacy')({
  component: () => <LegalPage page="privacy" title={m.title_privacy()} />,
});
