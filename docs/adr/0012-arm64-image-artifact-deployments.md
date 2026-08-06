# ARM64 image artifact deployments

## Status

Accepted

## Context

TechAtlas production runs on an ARM64 Oracle VM shared with another application. Building the
Rust services on that host competes with production workloads, and publishing release images to a
registry would require a registry credential on the VM. The host is intentionally configured with
only SSH ingress and a Cloudflare Tunnel for HTTP traffic.

## Decision

- The existing Quality workflow remains the required validation gate for pushes to `main`.
- A successful `main` Quality run triggers the production workflow. It uses a native GitHub-hosted
  ARM64 runner to build the backend, dashboard, and backup images for `linux/arm64`.
- The workflow saves the images as short-lived GitHub Actions artifacts, transfers them through the
  verified SSH connection, and loads them directly into Docker on the VM. No Docker registry
  credential is stored on the VM.
- Each release is pinned to its full Git commit SHA in `/etc/techatlas/release.env`. Production
  Compose accepts only these image references; it never builds application images on the VM.
- The remote release script refuses locally modified tracked deployment files, applies migrations
  before application services, waits for service health, verifies the loopback Caddy origin, and
  then enables the dedicated Cloudflare Tunnel.
- Releases are serialized. A failed migration or health check leaves the prior running services in
  place and requires an operator decision because migrations are forward-only.

## Consequences

GitHub Actions is the only production build environment and has an explicit ARM64 compatibility
check. The GitHub `production` environment needs only SSH host verification and the deployment
private key; database, Auth0, R2, and Cloudflare credentials remain host-only. Deployment
artifacts are retained for seven days for diagnosis, while Docker image cleanup remains a host
maintenance concern.

## Alternatives considered

- Build directly on the VM: rejected because it consumes scarce production CPU and memory.
- Pull images from GHCR or Docker Hub: rejected for the initial deployment because it requires a
  registry credential on the VM and adds a registry availability dependency to releases.
- Copy application source and run Compose builds over SSH: rejected because it is slower, less
  reproducible, and recreates the same host resource contention.
