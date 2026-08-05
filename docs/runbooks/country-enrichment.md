# Country enrichment

TechAtlas maps the approved public IP addresses used by a successful crawl through a local MaxMind
GeoLite2 Country database. Country means the resolved server or CDN IP location; it is not a
domain owner's incorporation, office, or audience location.

## Local setup

Create a MaxMind account and license key, then download the database without storing the key:

```bash
MAXMIND_LICENSE_KEY=your-license-key pnpm geoip:setup
COMPOSE_PROFILES=pipeline pnpm docker:up
```

The setup script stores the MMDB and release identifier in ignored `.techatlas/geoip/`, writes
only the absolute database path and release date to untracked `.env`, and never writes or prints
the license key. Re-run it when updating the MaxMind release, then recreate the worker. The
`pnpm docker:up` command deliberately clears any exported GeoIP overrides and explicitly loads
the root `.env` file, so local Compose reads the values written by setup.

After enabling GeoIP for an existing corpus, open **Admin → Scheduler** and use **Recrawl domains
missing country data**. The operation is authenticated, audited, and asks the scheduler to recrawl
only enabled domains with a successful snapshot but no country observation. It never changes an
immutable snapshot or bypasses crawl safety and robots policy.

Update the MMDB using the same command, then restart workers. Existing observations remain
immutable; a fresh crawl creates a new country observation and search update. Missing mappings are
stored as unavailable and do not block a successful crawl.
