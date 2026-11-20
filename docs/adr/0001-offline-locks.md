# Key locks are verified offline on the Mac

A key lock is opened with the current code from the single keyholder's authenticator app (TOTP), checked locally by the daemon against a root-only secret. There is no backend, account or network call in the unlock path, so Blocker works without us running user infrastructure and keyholders can unlock remotely by reading a code aloud.

## Consequences

- Against a user who knows an admin password, a key lock is friction, not a guarantee: root can read the secret or edit files. We compensate with self-healing config, a daemon outside the app bundle, and a tamper log the keyholder sees at the next unlock, and we tell users at setup that the keyholder should hold the admin password.
- A lost keyholder credential is recovered only by a 14-day witnessed-time recovery that downgrades key locks to self locks and is logged.

## Considered Options

- **Online keyholder approval** (push to the keyholder's phone, server-side tamper alerts): stronger, but needs accounts, a backend and notifications. Deferred, not rejected.
- **Passkeys**: unphishable and one tap, but cross-device use needs the keyholder physically near the Mac (Bluetooth) and a hosted associated domain. Planned as an alternative keyholder credential, chosen at enrollment.
- **Static unlock code**: learned by watching it typed once, then works forever.
