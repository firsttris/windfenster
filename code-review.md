# Code-Review Windfenster, 12.10.2026

Stand nach v0.1.8. Vier Prüfungen über die Kernbereiche, jede nur mit belegten Funden
(Datei:Zeile). **Bestätigt** = im Code nachvollzogen, **plausibel** = Verdacht, nicht bis zum
Ende verfolgt. Der Status sagt, was schon im Repo ist.

Der vierte Bericht (die Karte im Browser: Shader, Partikel, Leistung) läuft noch und wird hier
ergänzt.

## Zusammenfassung

Die Windberechnung ist korrekt: Einheiten (m/s → kn), Gitter-Ausrichtung, Längengrad-Konvention,
Datumsgrenze, Windrichtung, die Texturkodierung, das Abtasten der Spot-Zeitreihe (identisch mit
dem Shader), Verifikation, Mix, Ranking, Einigkeit, TAF-Auflösung, Windstatistik und der
METAR-Decoder wurden geprüft und stimmen.

Gefunden wurden:

- **2 echte Fehler mit sichtbarer Wirkung** (Sommerzeit-Tag leer; Böen fehlen, wenn die
  DWD-Datei später kommt);
- **4 Robustheitsfehler** (ein kaputter Schritt blockiert den Rest eines Laufs; ein abgestürzter
  Hintergrund-Loop bleibt tot; ein langsamer Dienst hält alle anderen Abrufe auf; ein leeres
  Modell wird 10 Tage nicht neu gefragt);
- **6 Leistungsbremsen**, davon zwei mit großer Wirkung (Karten-Kacheln ohne Cache; die
  Verifikation scannt dreimal die Stunde die ganze Messwert-Tabelle);
- eine Reihe kleiner Punkte.

Nichts davon betrifft die Richtigkeit der angezeigten Windwerte, bis auf die zwei Fehler oben.

## Bereits behoben (Commit 49bc9ca, Code-Review des Doku-Commits)

| Fund | Fix |
|---|---|
| Einigkeits-Kachel: Marken alle 20 kn ließen bei 70 kn die letzte Marke und die Einheit weg | Marken immer alle 10 kn |
| Screenshot-Test: Warten auf die Textur begann nach dem Laden der Seite | Warten beginnt vor dem Laden |
| TAF-Vergleichstest hatte zwischen 0 und 1 Uhr keine passende Spalte | TEMPO der Testdaten über fünf Stunden, Suche in allen Tagestabellen |
| Screenshot-Workflow: Commit mit GITHUB_TOKEN startet die Doku nicht | Workflow stößt `docs.yml` selbst an |
| Server-Build doppelt in `ci.yml` und `screenshots.yml` | eigener wiederverwendbarer Workflow `e2e-server.yml` |
| `exclude_docs` auf nicht existierende Datei, `WF_DEV_UI` undokumentiert | entfernt bzw. ergänzt |

## 1. Server-Windberechnung (`crates/wf-map`)

### Fehler

| # | Schwere | Fund | Status |
|---|---|---|---|
| 1.1 | mittel, bestätigt | **Ein kaputter Schritt blockiert den Rest des Laufs.** `service/icon_d2.rs:29-36`, `service/icon_global.rs:55-71`: ein Fehler bei einer Stunde (404, abgeschnittenes bz2) bricht `update_*` ab; alle späteren Stunden werden nicht geholt, `prune` läuft nicht. Alle 10 min scheitert es erneut an derselben Datei, bis der nächste Lauf kommt (3 h bei D2, 12 h bei ICON global). ECMWF (`service/ecmwf.rs:52-61`) ebenso. | **behoben** (a9fa098): Fehler je Stunde wird geloggt, die Schleife läuft weiter; ECMWF beendet nur die Runde |
| 1.2 | mittel, bestätigt | **Böen fehlen, wenn die Böen-Datei des DWD später gelistet wird.** `service/icon_d2.rs:16-33`, `service/icon_global.rs:40-54`: eine Stunde gilt als fertig, sobald U und V da sind; kommt `vmax_10m` Minuten später (der DWD schreibt die Variablen nacheinander, die Testdaten zeigen 06:44 und 06:47), bleibt die Stunde für den ganzen Lauf ohne Böen (Karte zeigt „Böe = Wind“). | **behoben** (a9fa098): `ready.rs`, eine Stunde wartet auf ihre Böe, außer Schritt 0 oder wenn eine spätere Böe schon gelistet ist |
| 1.6 | niedrig, plausibel | Die Tabelle der nächsten Dreieckszellen (`icon_global-grid.u32`) wird nur nach Dateigröße übernommen; ändert der DWD das Gitter, sind alle Texel falsch zugeordnet. `service/icon_global.rs:91-101`. | offen; Fix: Zellenzahl in der Datei speichern und bei Abweichung neu bauen |
| 1.7 | niedrig, plausibel | ICON-global-Ausschnitte können die Datumsgrenze nicht überspannen (`crop.rs:35-37`, `wind-map.ts:262-270`): dahinter zeichnet nur ECMWF. Für Europa egal. | offen, Hinweis |
| 1.8 | niedrig | Ausschnitte von ICON global behalten den weltweiten Wertebereich: bei 150 kn Spanne sind 8 Bit rund 0,6 kn je Stufe, bei 10 kn bis ~3° Richtung. Abwägung, kein Fehler. | offen |

### Leistung

| # | Schwere | Fund | Status |
|---|---|---|---|
| 1.3 | mittel (Pi), bestätigt | **Spot-Zeitreihe: ~2000 Thread-Wechsel je Aufruf.** `series.rs:125-141`: je Stunde `tokio::fs::read` des JSON, `File::open`, 4× seek+read, alles einzeln über den Blocking-Pool, ~190 Stunden nacheinander. Wird bei jedem Tippen auf die Karte und je Alarm aufgerufen. Fix: alles in einem `spawn_blocking` mit `std::fs`, Meta-JSON je Pfad cachen (Dateien sind unveränderlich). Erwartung: 10–50× weniger Overhead. | **behoben** (a9fa098, in der Commit-Nachricht nicht genannt): ein `spawn_blocking` mit `std::fs`; der Meta-Cache fehlt noch |
| 1.4 | mittel (Pi), bestätigt | **ICON global schreibt 1,5 GB `.rgba` je Lauf, hält ~3 GB.** 2880×1441×4 Byte × 93 Stunden, zwei Läufe; Alpha immer 255. Fix: nur ±80° Breite speichern (−11 %), ohne Alpha (−25 %), oder zeilenweise komprimiert; vom alten Lauf nur die Stunden behalten, die der neue noch nicht hat. | offen, Entscheidung |
| 1.5 | niedrig, bestätigt | `texture::pixels` kopiert jedes Feld einmal mehr in Knoten: bei ICON global ~130 MB Spitze je Stunde. Umwandlung in-place halbiert das. `texture.rs:110-113` | offen |
| 1.9 | trivial | ICON-D2 listet den Böen-Ordner, bevor klar ist, ob der Lauf überhaupt da ist (eine Anfrage je Tick zu viel). | **behoben** (a9fa098) |

### Als korrekt bestätigt

Scan-Modus des GRIB2 wird ausgewertet (Zeilen- und Spaltenrichtung), Zeile 0 ist Nord;
Längengrade 0..360 → −180..180, Wrap bei 1440×0,25°; `MS_PER_KN = 1852/3600`; Richtung
`(−u).atan2(−v)` = meteorologisch „woher“; Abtasten in `series.rs` ist Zeile für Zeile dasselbe
wie `sampleWind` im Browser und wie der Shader (Texel-Mitten, Wrap, Alpha-Gewichtung); Kodierung
R/G min..max, B 0..gust_max, A 0 außerhalb; Ausschnitt-Arithmetik exakt (Binärbrüche);
Nächste-Zelle-Suche in 0,5°-Eimern findet immer die nächste Zelle, wird einmal gebaut und
gespeichert; alles Rechenintensive in `spawn_blocking`; JSON wird zuletzt geschrieben, also nie
eine halbe Stunde gelistet; ECMWF-Range-Header, 429-Backoff, Schrittfilter; Index nimmt jede
Stunde aus dem neuesten Lauf, der sie hat.

Nicht prüfbar aus der Cloud: Dekodierung echter ICON-global-Dateien (Template 3.101) und
ECMWF-Packung gegen den Live-Dienst.

## 2. Vorhersage-Logik (`wf-core`, `wf-ingest`, `web/src/lib`)

### Fehler

| # | Schwere | Fund | Status |
|---|---|---|---|
| 2.1 | mittel–hoch, bestätigt | **Am Tag der Sommerzeit-Umstellung bleibt ein ganzer Tag leer.** `web/src/lib/forecast.ts:174-179`: `covers()` verlangt einen Wert für jede Stunde aus [2,5,…,23]; am Umstellungstag gibt es die Stunde 2 nicht, also „deckt“ kein Modell den Tag, und die Karte zeigt leere Spalten. Nachgestellt mit echtem `buildDays` (Europe/Berlin, 29.3.2026). Fix: nur die Stunden verlangen, die an dem Datum existieren. | **behoben**: `hoursOn` liest die Stunden des Tages aus der Uhr; am Umstellungstag 7 Spalten statt 8 |
| 2.2 | niedrig, bestätigt | Rundung aufs 0,01°-Raster unterscheidet sich bei negativen Koordinaten auf .xx5: `favorites.ts:12` (`Math.round`, halb nach +∞) gegen `geo.rs:138` (`f64::round`, halb weg von null). Ein gemerkter Spot auf der Südhalbkugel wird auf der Punktseite dann nicht erkannt. | offen |
| 2.4 | niedrig, bestätigt | TAF-Stunden zählen ab Ausgabezeit, nicht ab Abruf (`verification/run.rs:414-423`), anders als die Modelle. Kleine Abweichung von der eigenen Regel. | offen |
| 2.5 | niedrig, plausibel | Verifizierter Mix ≠ gezeigter Mix: Vorlauf-Bucket und 30-Tage-Fenster werden leicht anders berechnet (`ranking.rs:140-143` gegen `run.rs:448-477`). | offen |
| 2.6 | mittel, plausibel | **Ein Modell, das einmal leer antwortete, wird bis zu 10 Tage nicht neu gefragt.** `forecasts.rs:278-284` + `parse.rs:414-439`: eine vorübergehende Null-Antwort von Open-Meteo gilt als „außerhalb des Modellgebiets“ bis zum Löschen des Laufs. Nicht gegen den Live-Dienst prüfbar. | offen, Entscheidung |
| 2.7 | niedrig | Zellenbeschriftung rundet halb auf (13,6 → „14“), die Farbe nimmt den Rohwert (13,6 → Band 12): zwei Zellen „14“ können verschieden gefärbt sein. `units.ts:17-26`, `wind-cell.tsx:8` | offen |
| 2.8 | niedrig | METAR „1 1/2SM“ verliert die ganzen Meilen (`metar.ts:97-100`), nur US-Form. | offen |
| 2.9 | niedrig | `gearFor` teilt durch null, wenn `minKn === maxKn` (`gear.ts:29`). | offen |

### Leistung

| # | Schwere | Fund | Status |
|---|---|---|---|
| 2.3 | ~~mittel~~ **kein Fehler** | Die Verifikation scannt **nicht** die ganze `observation`-Tabelle: EXPLAIN QUERY PLAN zeigt Stationen-Scan mit Primärschlüssel-Abfrage je Station. Die zusätzlichen Abfragen für Stationen ohne Vorhersage sind je ein Index-Treffer ins Leere (~0,3 s pro Stunde). Eine Abkürzung über die `place`-Tabelle wurde verworfen: Serien hängen nicht zwingend an einem `place`-Eintrag (Test `verifies_a_day`). | **erledigt, nichts zu tun** |

### Hinweise (keine Fehler)

- Überlappende Läufe zählen alle: `n` im Ranking ist „Vorhersagen verglichen“, nicht Stunden.
  Mittelwerte bleiben vergleichbar.
- Vorlauf zählt ab Lauf-Initialisierung; Bucket 0 hat bei Modellen nur ~15–22 h nutzbaren Vorlauf.

### Als korrekt bestätigt

Winkelfehler modulo 360, Richtung erst ab 5 kn, ≥14-kn-Trefferkreuz, Buckets, UTC-Tage,
Gewichte 1/MAE² mit 0,5-kn-Boden (keine Division durch null), gleiche Gewichte bis 14 Tage,
Mix (Vektormittel, Kalme → keine Richtung), Ranking-Regeln, Serien-Kodierung i16, Haversine,
Einheiten; Open-Meteo-Aufruf (`wind_speed_unit=kn`, 10 m, `timeformat=unixtime`), Budget
konservativ, 5/s; Windstatistik (Sonnenzeit, 9–18 Uhr, windiger Tag, 16 Sektoren); TAF-Auflösung
(FM, BECMG ab Mitte, TEMPO/PROB nur Spanne); TS: `localClock`, Herbst-Wiederholung, `isNight`,
`barShare`, `mainDirection`, `spotWorks`, `spreadAt`, `bestTime`, `inSector`, `nearest` ±20 min,
METAR-Decoder (VRB, Kalme, Böen, MPS/KMH, CAVOK, M05, Altimeter).

## 3. API, Datenbank, Scheduler, Docker

### Robustheit

| # | Schwere | Fund | Status |
|---|---|---|---|
| 3.2 | mittel, bestätigt | **Ein langsamer Dienst hält alle anderen Abrufe auf.** `wf-ingest/src/scheduler.rs:37-124`: eine Schleife, strikt nacheinander, nur das 60-s-Timeout je Anfrage. Hängt der DWD, sind bei 30 Stationen bis zu 60 min keine METARs, keine Läufe, keine Alarm-Messwerte. Fix: `tokio::time::timeout` je Schritt oder Messungen und Vorhersagen in zwei Tasks. | **behoben**: Zeitgrenze je Schritt (2/5/10/15 min), aufgegebene Schritte laufen in der nächsten Runde |
| 3.3 | mittel, plausibel | **Ein Panic in einem Hintergrund-Loop beendet ihn still.** `main.rs:365-383`: die fünf `tokio::spawn` werden nicht überwacht; nach einem Panic (z. B. vergifteter Mutex) liefert der Server alternde Daten, Health bleibt grün. Fix: kleiner `supervise`-Wrapper, der nach einem Panic neu startet; „letzter erfolgreicher Tick“ im Health. | **behoben**: `supervise.rs` startet jeden Loop nach einem Panic nach 60 s neu (Test); Health-Anzeige offen |
| 3.6 | mittel (DoS), plausibel | Passwort-Anmeldung: keine Grenze für gleichzeitiges Argon2 (19 MiB je Hash, Blocking-Pool bis 512 Threads); Rate-Limit nur je Adresse. Fix: Semaphore (~4) um hash/verify, bei Erschöpfung 503. | offen |
| 3.7 | niedrig–mittel | Live- und Verlaufs-Handler holen im Request-Pfad mit 60-s-Timeout (TAF + METAR nacheinander). Fix: 10–15 s für Request-Pfad-Abrufe. `live.rs:73-80` | offen |
| 3.13 | niedrig, bestätigt | `terrain.pmtiles` ohne ETag (`ServeFile` sendet keinen); der Kommentar in `map.rs:176-180` verlässt sich darauf. Nach der jährlichen Erneuerung liest ein offener Client falsche Offsets bis zum Neuladen. Fix: ETag aus mtime+Größe. | offen |

### Leistung

| # | Schwere | Fund | Status |
|---|---|---|---|
| 3.1 | mittel–hoch, bestätigt | **Karten-Kacheln: Verzeichnisse der PMTiles werden je Kachel neu gelesen und entpackt.** `basemap/serve.rs:68`: `try_from_source` = `NoCache`; bei Überzoom bis zu 7 Lookups je Kachel über alle Spot-Dateien, plus `read_dir` + `metadata` je Anfrage. Ein Schwenk = Hunderte Verzeichnis-Reads. Fix: `HashMapCache` (wie `fetch.rs:140` schon beim HTTP-Reader), Spot-Liste ~10 s cachen. Erwartung: von mehreren ms + CPU je Kachel auf unter 1 ms. | **behoben**: Verzeichnis-Cache je Archiv, Spot-Liste und Datei-Stand höchstens alle 10 s von der Platte (neue Spots sofort); der Leser hält seine Datei offen, damit Verzeichnis und Daten nach dem Erneuern nie gemischt werden (Test); nicht gemessen |
| 3.4 | mittel (Alarme) | `/api/map/series` und die Alarm-Runde: ~200 Dateien je Aufruf (siehe 1.3); `index()` liest je Aufruf alle Lauf-Ordner; 1000 Alarme ≈ 170 000 Datei-Öffnungen alle 10 min. Fix: `MapIndex` 10–30 s cachen, Meta-JSON cachen, in der Alarm-Runde `series` je gerundetem Ort nur einmal. | offen |
| 3.5 | niedrig–mittel | Forecast-Cache nach Einträgen begrenzt (20 000), nicht nach Bytes: im Extrem 1,3 GB, praktisch ~200 MB. Fix: `weigher` wie bei `map_piece.rs` (dort 256 MB, evtl. senken). | offen |
| 3.8 | niedrig | `meta.rs:67` parst die Sprachdatei bei jedem Stations-Seitenaufruf (0,3–0,5 ms). Fix: `LazyLock`. | offen |
| 3.9 | niedrig | SPA-Bundle wird je Besucher live mit Brotli 6 komprimiert (`natural-earth` 6,7 MB, `map.js` 1,1 MB: 0,3–0,6 s CPU je kaltem Besucher). Fix: im Vite-Build vorkomprimieren oder gzip für Bodies > 1 MB. | offen, Entscheidung |
| 3.10 | niedrig | `KEPT_IDS`-Subquery scannt `favorite` je Station (stündlich, 10k × 1k). Fix: Index `favorite(slug)`. `station.rs:140-144` | offen |
| 3.11 | niedrig | `DELETE FROM verification_daily WHERE day < ?` scannt stündlich die ganze Tabelle (~100 ms bei 1,7 Mio.). Index auf `day`. Andere Sweeps klein. | offen |
| 3.12 | niedrig | Speicherung der Vorhersage-Läufe (10 Tage) nur durch das Open-Meteo-Budget begrenzt: ~90 MB im Normalfall, mit API-Key skaliert es (1000 heiße Orte ≈ 900 MB). | offen, Hinweis |
| 3.14 | niedrig | Textur-PNGs werden je Anfrage ganz in den Speicher gelesen (1–2 MB); `ServeFile` würde streamen. `map.rs:210` | offen |
| 3.15 | niedrig | `FROM rust:1-trixie` gleitet auf die neueste Rust-Version; für reproduzierbare Releases Minor pinnen. | offen |

### Als korrekt bestätigt

SQLite: WAL, `synchronous=NORMAL`, `foreign_keys`, `busy_timeout` 10 s, Pool 8; alle heißen
Abfragen mit EXPLAIN QUERY PLAN geprüft und über Indizes bedient (Messungen, Serien,
Verifikation je Station, Sessions, Favoriten, Pins, nächste Station/Pegel, TAF-Purge); alle
Fremdschlüssel mit Kind-Indizes; Transaktionen kurz; Caches alle begrenzt und mit TTL;
Forecast-Cache-Invalidierung korrekt; reqwest mit Timeouts, rustls, gzip; Open-Meteo-Limiter und
Backoff; ECMWF-429-Behandlung; Klima-Abruf gedrosselt; Map-Loops überlappen sich nicht, ICON
global im eigenen Task; Alarm-Runde liest je Job jede Stunde nur einmal; Body-Limit 2 MB; kein
CORS (same-origin) richtig; Cookies `HttpOnly; SameSite=Lax; Secure` bei https; Docker: tini,
schlankes Laufzeit-Image, Healthcheck, sauberes Herunterfahren bei SIGTERM; `/api/health`
antwortet sofort.

## 4. Karte im Browser (Shader, Partikel, Leistung)

Bericht steht noch aus.

## Bewertung der offenen Punkte (am Code nachgeprüft)

Jeder offene Punkt wurde gegen den Code gehalten: stimmt er, und lohnt sich der Fix (Nutzen
gegen Aufwand und Risiko)? Kein neuer Cache, wo sich Daten ändern können.

| # | Stimmt? | Lohnt sich? | Begründung |
|---|---|---|---|
| 2.6 | ja | **ja, behoben** | Ein einmaliger Aussetzer kostet 10 Tage ohne das Modell am Ort. Fix: „leer“ gilt einen Tag, dann einmal neu fragen; kostet höchstens eine Modell-Abfrage pro Ort und Tag mehr. |
| 3.7 | ja | **ja, behoben** (nur Live-Kachel) | Der erste Besucher einer veralteten Station wartet bis zu 2 min, wenn ein Dienst hängt. Fix: 15 s Grenze für den Abruf im Request; der Scheduler holt den Rest. Verlauf-Nachladen bleibt (große Dateien). |
| 3.6 | ja | **ja, behoben** (öffentliche Instanz geplant) | Viele parallele Passwort-Anmeldungen belegen CPU und je 19 MiB. Fix: höchstens 4 gleichzeitige Hashes. |
| 2.4 | ja | **ja, behoben** | Eine Zeile, macht die TAF-Wertung konsistent mit der eigenen Regel (nur Stunden nach dem Abruf). |
| 2.7 | ja | **ja, behoben** | Zwei Zellen „14“ in verschiedenen Farben sehen nach Fehler aus. Fix: Farbe nach dem gerundeten Wert, zentral in `windBand`. |
| 2.2 | ja | **ja, behoben** | Selten (Südhalbkugel, genau .xx5), aber eine Zeile: gleich runden wie der Server. |
| 2.9 | ja | **ja, behoben** | Eine Zeile Schutz gegen Division durch null. |
| 1.4 | ja | später, **Entscheidung** | 3 GB für ICON global sind auf einem Pi viel; `WF_ICON_GLOBAL=false` hilft schon heute. Ein anderes Speicherformat ist ein größerer Umbau. |
| 3.9 | ja | **ja, behoben** (auf Wunsch) | Spürbar nur für den ersten Besuch (Bundle wird live komprimiert, auf dem Pi 1–2 s). Fix braucht Vorkomprimieren im Build oder beim Start. |
| 3.13 | ja | nein, vorerst | Betrifft nur eine offene Karte genau beim jährlichen Erneuern des Reliefs; Neuladen behebt es. |
| 3.5 | ja | nein, vorerst | Realistisch ~200 MB; ein Gewicht nach Größe wäre schöner, ändert aber nichts Spürbares. |
| 1.5 | ja | nein | 130 MB Spitze je ICON-global-Stunde sind auch auf einem Pi 4 tragbar. |
| 1.6 | teils | nein | Der DWD hat das Gitter seit 2015 nicht geändert; bei einer Änderung zeigen sich falsche Werte deutlich. |
| 1.7, 1.8, 2.5 | ja | nein | Datumsgrenze, Quantisierung, Mix-Bucket: für Europa ohne spürbare Wirkung. |
| 2.8 | ja | nein, vorerst | Nur US-Sichtweiten wie „1 1/2SM“. |
| 3.4, 3.8, 3.10, 3.11, 3.14 | ja | nein | Je unter einer Sekunde pro Stunde oder Millisekunden je Anfrage bei der erwarteten Größe. |
| 3.12, 3.15 | – | nein | Hinweise; CI nutzt bewusst die neueste Rust-Version. |

## Vorschlag für die Reihenfolge (ursprünglich)

1. **Jetzt, klein und klar:** 2.1 (Sommerzeit); 3.1
   (Kachel-Cache); 2.3 (Verifikations-Scan); 3.2 (Timeouts je Schritt); 3.3 (Loops überwachen);
   3.10, 3.11 (zwei Indizes, eine Migration); 2.2, 2.4, 2.8, 2.9, 3.8 (Kleinigkeiten).
2. **Mit Entscheidung:** 1.4 (Speicher von ICON global), 2.6 (leere Modellantwort), 3.6
   (Argon2-Grenze), 3.9 (Vorkomprimieren), 3.5 (Cache-Gewicht).
3. **Später / Hinweis:** 1.6, 1.7, 1.8, 2.5, 2.7, 3.7, 3.12–3.15.
