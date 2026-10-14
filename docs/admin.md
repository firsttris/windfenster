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
