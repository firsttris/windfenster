import { Check, Share2 } from 'lucide-react';
import { useState } from 'react';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';

/** Shares the view as it is (place, zoom, hour, layer, model are all in the address): the
 * phone's share sheet where there is one, else the link to the clipboard. */
export function ShareButton({ className }: { className?: string }) {
  const [copied, setCopied] = useState(false);
  const share = async () => {
    const url = window.location.href;
    if (navigator.share) {
      await navigator.share({ title: document.title, url }).catch(() => {});
      return;
    }
    await navigator.clipboard?.writeText(url);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label={copied ? m.map_copied() : m.map_share()}
      title={copied ? m.map_copied() : m.map_share()}
      onClick={() => void share()}
      className={className}
    >
      {copied ? <Check aria-hidden="true" /> : <Share2 aria-hidden="true" />}
    </Button>
  );
}
