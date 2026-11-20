# Blocklist licences for the adult, gambling and bypass categories

Answers [#9](https://github.com/devinsba/blocker-apps/issues/9) (plan Phase 5.1):
which upstream lists may `blocker-server` fetch, compile into a per-category
FST, sign and serve to every client of a free macOS app?

Checked 2026-11-20 against each project's own repo, LICENSE file and README.
Counts are what each project reported that day. **This is not legal advice.**
It is an engineering reading of the licences. Get a lawyer to look at it before
a commercial or large-scale release.

## Recommendation

| Category | Use | Optional extra | Licence of the compiled file |
|---|---|---|---|
| `adult` | HaGeZi NSFW + OISD NSFW | StevenBlack `porn-only` | GPL-3.0 |
| `gambling` | HaGeZi Gambling (full) | UT1 `gambling`, StevenBlack `gambling-only` | GPL-3.0 |
| `bypass` | HaGeZi DoH/VPN/Tor/Proxy Bypass (complete edition) | UT1 `vpn`, `doh` | GPL-3.0 |

Every recommended list allows redistributing a modified, compiled copy. The
main ones (HaGeZi, OISD) are **GPL-3.0**, so each compiled category file is a
GPL-3.0 work. MIT, CC BY 4.0 and CC BY-SA 4.0 inputs can all be merged into
it: MIT and CC BY 4.0 are GPL-compatible, and Creative Commons
[declared CC BY-SA 4.0 one-way compatible with GPLv3](https://creativecommons.org/share-your-work/licensing-considerations/compatible-licenses/).

### What we have to do

1. **Licence each compiled category file as GPL-3.0** and ship the GPL-3.0
   text with it. HaGeZi says outright that "a copy of the license ... has to
   accompany any redistribution"
   ([README, Disclaimer](https://github.com/hagezi/dns-blocklists#disclaimer)).
   Serve `COPYING` next to the files, or point to it from the manifest.
2. **Provide the Corresponding Source** (GPL-3.0 §6). The FST is a
   machine-readable form. Its "preferred form for making modifications" is the
   plain domain list plus the tools that build it. Either serve the merged
   plain-text domain list for each published build next to the FST, or put up
   a written offer. `blocker-server` is MIT
   (`Cargo.toml`, `license = "MIT"`), which is GPL-compatible. Publishing it
   covers the build-tool side, as long as the repo is public when we ship.
3. **Keep notices and credit the sources.** Show an Acknowledgements screen in
   the app and on the server's index page. It names each list, its author, its
   licence and a link, and says we modified it (merged, deduplicated, filtered
   by the guard). CC BY-SA 4.0 (UT1) and CC BY 4.0 (tiuxo, part of
   StevenBlack's porn extension) require that credit.
4. **Record the licence in `sources.toml`** (plan 5.1). Note that
   `RawCategory` is `#[serde(deny_unknown_fields)]`
   (`crates/blocker-server/src/sources.rs`), so adding a `license` or
   `attribution` field needs a small code change.

### Is the app itself GPL?

Probably not. The app reads a data file at runtime. It does not link or
include GPL code, so the usual reading is that the FST and the app are
separate works. The GPL obligations then attach to the category files we
convey, not to the app binary. This is the reading most worth a lawyer's look,
because the app is distributed for free to the public and is not GPL itself.

## Per-list findings

### HaGeZi DNS Blocklists (recommended)

- Repo: <https://github.com/hagezi/dns-blocklists>. **GPL-3.0** (LICENSE file
  and README badge). Actively maintained: last push 2026-11-20, about 29k stars.
- Redistributing a modified copy is allowed "only under the terms of that
  license", and the licence text must accompany it. The README also warns
  that GPL "doesn't hand you any rights in the upstream data itself" for
  third-party inputs. That only matters if we pull HaGeZi's upstreams
  directly. We don't. ([Disclaimer](https://github.com/hagezi/dns-blocklists#disclaimer))
- Other terms in the disclaimer: the lists may be suspended "for excessive
  query volume or abusive ... use". One server fetch per day is fine; clients
  never fetch HaGeZi directly. The deployer is responsible for the lawful use
  of restrictive lists (NSFW, gambling, bypass). Governed by German law.
- Cadence: GitHub/jsDelivr publish once a day at 11:00 UTC. A build mirror
  publishes every 4 to 8 hours.
  ([Update Interval](https://github.com/hagezi/dns-blocklists#mirrors))
- Formats: Adblock, dnsmasq, wildcard (asterisk and plain domains), RPZ. Use
  the `wildcard/*-onlydomains.txt` files: one domain per line, and each entry
  also blocks its subdomains.

| List | Entries | URL (plain domains) |
|---|---|---|
| NSFW | 87,887 | `https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/wildcard/nsfw-onlydomains.txt` |
| Gambling (full) | 683,160 | `.../wildcard/gambling-onlydomains.txt` |
| Gambling medium / mini | 105,273 / 52,534 | `.../wildcard/gambling.medium-onlydomains.txt`, `gambling.mini-onlydomains.txt` |
| DoH/VPN/Tor/Proxy Bypass (complete) | 16,618 | `.../wildcard/doh-vpn-proxy-bypass-onlydomains.txt` |
| DoH only | 3,570 | `.../wildcard/doh-onlydomains.txt` |

The "medium" and "mini" gambling lists exist for resource-limited ad blockers.
An FST handles 683k domains easily, so use the full list. HaGeZi also publishes
a DoH IP list (`ips/doh.txt`). It is not a domain list, so it does not fit the
FST. It may be useful later if the filter blocks by IP.

### OISD NSFW (recommended for adult)

- Repo: <https://github.com/sjhgvr/oisd>, site <https://oisd.nl>. **GPL-3.0**
  (LICENSE file, and the FAQ answers "Under which license are the oisd lists
  distributed? GNU General Public License v3.0":
  <https://oisd.nl/faq>). Maintainer: Stephan van Ruth. Last push 2026-11-20.
- The list header repeats the licence: `# License: https://github.com/sjhgvr/oisd/blob/main/LICENSE`.
  No extra terms beyond the GPL.
- Cadence: "At least once every 24 hours". Dead domains are pruned every 5 days
  (FAQ). The header says `Expires: 1 hours`.
- Size: `domainswild2_nsfw.txt` has 498,273 entries. `nsfw_small` keeps only
  the entries that are also in the Tranco top 1M.
- Format: `domainswild2_nsfw.txt` is plain domains, and each entry also blocks
  its subdomains. Source:
  `https://raw.githubusercontent.com/sjhgvr/oisd/main/domainswild2_nsfw.txt`
  (or `https://nsfw.oisd.nl/domainswild2`).

### StevenBlack hosts extensions (optional)

- Repo: <https://github.com/StevenBlack/hosts>. The repo is **MIT**
  (`license.txt`). Actively maintained: last commit 2026-11-15.
- The extensions are mirrored from other projects, and each keeps its own
  licence, as listed in its `extensions/<cat>/<src>/update.json`:
  - porn: bigdargon (MIT), brijrajparmar27 (CC BY 4.0), clefspeare13 (MIT),
    sinfonietta (MIT), sinfonietta-snuff (MIT), tiuxo (CC BY 4.0).
  - gambling: bigdargon (MIT), sinfonietta (MIT).
  All are permissive or attribution-only, so all are fine to merge into a
  GPL-3.0 file.
- **Use only the `*-only` alternates**, for example
  `alternates/porn-only/hosts` (76,811) and `alternates/gambling-only/hosts`
  (6,744). The "Unified hosts + ..." files also include the base ad/malware
  sources, and some of those are non-commercial (MVPS: CC BY-NC-SA 4.0;
  someonewhocares: "non-commercial with attribution"; per the
  source table in the [README](https://github.com/StevenBlack/hosts/blob/master/readme.md)).
  Non-commercial terms are not GPL-compatible.
- Format: hosts file (`0.0.0.0 domain`). The server parser has to strip the IP.
- The upstreams update only "occasional[ly]" (`frequency` in update.json).
  brijrajparmar27 was last pushed in 2021, and the Clefspeare13 repo is now
  "access blocked" by GitHub (StevenBlack carries a frozen copy). HaGeZi and
  OISD already merge most of this, so the added coverage is small.

### UT1 (Université Toulouse Capitole) blacklists (optional supplement)

- Mirror: <https://github.com/olbat/ut1-blacklists>, synced daily from the
  university site. Maintainer: Fabrice Prigent. The official page
  (`dsi.ut-capitole.fr/blacklists`) showed "Site en maintenance" on 2026-11-20.
- Licence: "available to download and use under the condition of the Creative
  Commons BY-SA" 4.0 licence (mirror README). GitHub detects no licence file.
  Attribution is required (credit, licence link, note changes). ShareAlike is
  satisfied by GPLv3 through CC's compatibility declaration.
- Relevant categories (entry counts from the mirror README): `adult`
  4,600,371; `gambling` 47,823; `vpn` 6,039; `doh` 3,016; `redirector`
  132,616; `residential-proxies` 138.
- Format: a directory per category with a `domains` file (plain domains) and a
  `urls` file. Use `domains` only. Large files are gzip-compressed.
- Caveats: `adult` is very large and old, with many dead domains. It would
  multiply the FST size for little gain over OISD, so skip it. `redirector` is
  too broad for `bypass` (it includes translation and cache services), so skip
  it as well. `vpn` and `doh` are reasonable cross-checks for `bypass`.

### Rejected

- **Block List Project** (<https://github.com/blocklistproject/Lists>):
  Unlicense, so no obligations at all. But GitHub shows the repo as
  **archived** (last push 2026-10-27), so it will not be updated. Porn ~953k
  and gambling ~279k entries. Do not depend on it.
- **1Hosts** (<https://github.com/badmojr/1Hosts>): MPL-2.0. It publishes only
  privacy/ad tiers (Lite, Pro, Xtra) and no adult, gambling or bypass lists.
- **chadmayfield/my-pihole-blocklists**: GPL-3.0, archived in 2021.
- Anything with a non-commercial clause (MVPS, someonewhocares): it conflicts
  with GPL-3.0 when merged, and with any future paid tier.

## Suggested `sources.toml`

```toml
[[category]]
name = "adult"
description = "Pornography and other adult content"
sources = [
  "https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/wildcard/nsfw-onlydomains.txt",   # HaGeZi, GPL-3.0
  "https://raw.githubusercontent.com/sjhgvr/oisd/main/domainswild2_nsfw.txt",                    # OISD, GPL-3.0
]

[[category]]
name = "gambling"
description = "Online gambling and betting"
sources = [
  "https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/wildcard/gambling-onlydomains.txt", # HaGeZi, GPL-3.0
]

[[category]]
name = "bypass"
description = "VPN, proxy, Tor and encrypted-DNS services"
sources = [
  "https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/wildcard/doh-vpn-proxy-bypass-onlydomains.txt", # HaGeZi, GPL-3.0
]
```

The licences are comments here until `sources.rs` accepts a field for them.
Plan 5.2 still applies: the guard must reject anything it protects. The
bypass list in particular names VPN providers whose control planes the guard
keeps reachable.
