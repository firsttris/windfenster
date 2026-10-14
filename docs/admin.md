---
description: The admin page of Windfenster - who may open it, how the first admin is made, and what each of its sections shows and changes.
---

# The admin page

Under `/admin` an admin sees what the server is doing and changes it without a restart. The
page is only for accounts with the admin role; everyone else gets "only for admins".

## The first admin

Nobody can make themselves an admin over the web, not even the first account. On the server:

```bash
windfenster admin tristan@example.org      # by e-mail address, by name, or by #id
windfenster admin --list                   # who is admin
windfenster admin --remove tristan@example.org
```

In the container: `docker compose exec windfenster windfenster admin tristan@example.org`
(Podman: `podman exec windfenster windfenster admin …`). The account must exist: sign up first.
Once there is an admin, they can make more on the admin page. The last admin who may sign in
keeps the role: it cannot be taken, and the account cannot be disabled or deleted.

Every change an admin makes is written to the admin log (who, when, what), kept a year.

## Overview

What needs a decision comes first: a missing or plain-http `WF_PUBLIC_URL`, requests without the
client-address header (`WF_CLIENT_IP_HEADER`; without a proxy in front that sets it, anyone can
set it and pass the limits per visitor), a task whose last run failed, a source that failed three
times or more in a day, a loop that crashed and was started again, an Open-Meteo budget used to
80 %, less than 2 GB free. Then the day's numbers: Open-Meteo calls against the budget, the data
folder on the disk, accounts, alarm messages.

Every background task with how often it runs, when it last ran, how long it took and what came of
it (with the error as the log has it), and a button that runs it now:

| Task | Every | |
|---|---|---|
| New model runs, refresh places | 5 min | Open-Meteo |
| METAR, water gauges | 9 min | aviationweather.gov, PEGELONLINE |
| TAF, DWD 10-minute values | 29 min | aviationweather.gov, DWD |
| Cleanup and verification | hourly | the database |
| Station lists | weekly (checked hourly; "now" fetches them whatever their age) | OurAirports, aviationweather.gov, DWD |
| Wind map ICON-D2 and ECMWF, ICON global | 10 min | DWD, ECMWF |
| Basemap and relief | daily | Protomaps, Mapterhorn |
| Detail spots | 1 min | Protomaps |
| Alarms | 10 min | push services, SMTP |

Each outside source shows when it last answered and how often it failed in the last 24 hours.
The state lives in memory and fills again within a round after a restart.

## Users

The list of accounts with how each signs in (passkey, password), when it was last seen (to the
day), its spots and alarms, and its role. Picking one shows its sessions, push devices, gear and
the alarms delivered to it in the last 30 days. An admin can

- send the confirmation mail again,
- sign the account out everywhere,
- download the account's data as JSON (for its owner: GDPR); it holds no password hash and no
  passkey keys,
- make it an admin or take the role,
- disable it: no sign-in, its sessions end, its alarms rest, its data stays; or enable it again,
- delete it with everything it holds.

Nobody sees passwords, admins neither.
