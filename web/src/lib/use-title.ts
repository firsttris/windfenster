import { useEffect } from 'react';

/** Sets the tab title while the page is shown. */
export function useTitle(title: string | null) {
  useEffect(() => {
    if (title) document.title = title;
  }, [title]);
}
