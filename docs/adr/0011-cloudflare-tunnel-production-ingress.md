# Cloudflare Tunnel production ingress

## Status

Accepted

## Context

TechAtlas shares a 2-vCPU Oracle VM with another application. The VM deliberately accepts only
SSH inbound and already runs an independent Cloudflare Tunnel. Exposing TCP 80 and 443 would add
public firewall rules, require direct certificate management at the host edge, and create an
unnecessary port-sharing concern.

## Decision

- Production traffic enters through a dedicated Cloudflare Tunnel and a Cloudflare-managed public
  hostname.
- Cloudflare terminates public TLS. A dedicated `cloudflared` systemd service forwards the
  hostname to TechAtlas Caddy at `http://127.0.0.1:8081`.
- Caddy remains the origin router for the dashboard and `/api/*`, but serves HTTP only and is bound
  to loopback. It does not obtain ACME certificates.
- VM and Oracle ingress keep TCP 80 and 443 closed. PostgreSQL, Redis, Meilisearch, telemetry,
  and Grafana remain private as before.
- The TechAtlas tunnel has its own credentials and systemd unit; it must not share or alter the
  existing Wavesend tunnel configuration.

## Consequences

The public hostname must exist in a Cloudflare-controlled zone before the tunnel is enabled.
Cloudflare Tunnel credentials become host-only secrets. Cloudflare availability becomes part of
the public ingress dependency, while the origin host has a smaller exposed network surface.

## Alternatives considered

- Direct Caddy TLS on 80 and 443: rejected because the host intentionally uses Cloudflare Tunnel
  and does not expose public web ports.
- Add TechAtlas to the Wavesend tunnel: rejected to preserve independent routing, credentials, and
  operational ownership.
