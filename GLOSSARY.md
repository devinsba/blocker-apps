# Blocker

A website and app blocker for the general public, aimed first at people who want
to keep adult content and gambling out of reach, with scheduled blocking of
distractions like social media alongside.

## Rules

**Rule**:
One thing the user configures: what to block (categories, domains, apps), when
(its schedule), and how hard it is to undo (its lock). Rules cover the whole Mac,
every account on it.
_Avoid_: Block (as a noun), filter, policy

**Category**:
A named, server-maintained list of domains, such as Adult, Gambling or Bypass
(VPN, proxy and anonymizer services), that a rule can block as a unit.
_Avoid_: List, blocklist (when meaning the category)

**Schedule**:
When a rule applies: either always, or during weekly time windows.
_Avoid_: Timer, window (for the whole schedule)

**Active**:
A rule is active when its schedule covers the current time.
_Avoid_: On, running

**Blocking**:
The state where at least one rule is active.
_Avoid_: Enforcing, protected

**Browser profile**:
The browser settings Blocker installs so covered browsers reveal the sites they
visit, and optionally enforce SafeSearch.
_Avoid_: Policy, configuration profile (as the domain term)

**SafeSearch**:
An option that forces safe results in search engines and YouTube in covered
browsers, and blocks browsers that are not covered.
_Avoid_: Safe mode, restricted mode (as the general term)

**Covered browser**:
A browser that Blocker can configure: through the browser profile, or for
Safari, through Screen Time's web content filter.
_Avoid_: Managed browser, supported browser

**Setup wizard**:
The first-run flow that walks the user through every manual step Blocker
needs: approving the filter, the first rule, the keyholder, and SafeSearch.
_Avoid_: Onboarding (as a name), installer

## Locks

**Lock**:
The part of a rule that decides how hard it is to change or remove. Every rule has
exactly one: none, self lock, or key lock.
_Avoid_: Locked (as a yes/no flag), protection level

**Self lock**:
A lock the user holds against themselves: changing the rule waits out a cooldown.
_Avoid_: Soft lock, personal lock

**Cooldown**:
The delay between asking to change a self-locked rule and the change taking
effect. It can be lengthened, never shortened, and only time Blocker was running
to witness counts toward it.
_Avoid_: Timeout, grace period

**Key lock**:
A lock where changing the rule requires an unlock code held by a keyholder.
_Avoid_: Hard lock, password lock

**Keyholder**:
The one person per Mac, other than the user, who can open key locks, such as an
accountability partner or a parent. Replacing the keyholder requires the current
keyholder's unlock code.
_Avoid_: Admin, supervisor, partner (as the general term)

**Unlock code**:
The short-lived code shown by the keyholder's authenticator app, entered to open
key locks. The enrollment QR code is the keyholder's backup.
_Avoid_: Password, PIN, key (alone)

**Unlock window**:
The period, chosen by the keyholder at each unlock, during which key-locked rules
can be weakened. Changes made in it persist after it closes.
_Avoid_: Session, grace period

**Pending change**:
A request to weaken a self-locked rule that is waiting out its cooldown or its
re-confirmation. At most one per rule; it lapses if not re-confirmed.
_Avoid_: Request (alone), queued edit

**Weaken / Strengthen**:
Any change that makes blocking less / more effective. Strengthening applies at
once; weakening must pass the rule's lock.
_Avoid_: Loosen, relax, tighten

**Recovery**:
The path for a user whose keyholder can no longer produce an unlock code: a long
cooldown after which the keyholder is removed and key locks become self locks.
_Avoid_: Reset, override

**Tamper**:
Any attempt to weaken blocking other than through a rule's lock, such as editing
files, disabling the filter, deleting the app, or changing the clock.
_Avoid_: Hack, violation

**Tamper log**:
The record of attempts to bypass locks, shown to the keyholder at their next
unlock.
_Avoid_: Audit log, violations
