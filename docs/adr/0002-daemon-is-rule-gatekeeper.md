# The root daemon is the only gatekeeper for rule changes

The UI and CLI no longer write `config.toml` through an administrator prompt; they ask the root daemon over a local socket, and the daemon applies each change according to the rule's lock: strengthening applies at once without admin rights, weakening a lock-free rule needs admin, weakening a self-locked rule becomes a pending change that waits out a cooldown of daemon-witnessed time and must be re-confirmed, and weakening a key-locked rule needs an unlock window opened by the keyholder. Enforcing locks in a CLI the user runs as root cannot work, and wall-clock time is trivially changed, so the daemon owns both the rules and the clock it trusts.

## Consequences

- A non-admin user (for example a child) can add or tighten rules but not weaken them.
- Edits to `config.toml` that bypass the daemon are treated as tampering and reverted for locked rules.
- Uninstalling is a weakening of every rule and passes the strongest lock on the Mac.
