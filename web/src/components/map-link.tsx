/** "Show on map": the wind map at the place, zoomed in, with the place picked (as Windfinder's
 * "Auf Karte anzeigen"). */
import { Map as MapIcon } from 'lucide-react';
import { type MapSpot, mapHref } from '@/lib/map-url';
import { m } from '@/paraglide/messages';
import { Button } from '@/ui/button';

export function MapLink(spot: MapSpot) {
  return (
    <Button variant="outline" asChild>
      <a href={mapHref(spot)}>
        <MapIcon aria-hidden="true" />
        {m.place_showOnMap()}
      </a>
    </Button>
  );
}
