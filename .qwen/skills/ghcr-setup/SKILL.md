---
name: ghcr-setup
description: Configure GitHub Actions to publish Docker images to GHCR (GitHub Container Registry)
source: auto-skill
extracted_at: '2026-06-01T09:38:30.428Z'
---

# Publishing Docker Images to GHCR

GHCR (GitHub Container Registry) is GitHub's native container registry. Unlike Docker Hub, it requires **no external secrets** — just the built-in `GITHUB_TOKEN`.

## Minimal workflow for GHCR

```yaml
name: Docker Image CI

on:
  push:
    branches: [main]
  release:
    types: [published]

permissions:
  contents: read
  packages: write  # ← Required for GHCR push

jobs:
  docker:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=raw,value=latest,enable={{is_default_branch}}
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=sha

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build and push
        uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
```

## Dual registry (Docker Hub + GHCR)

To push to both registries in one build:

```yaml
permissions:
  contents: read
  packages: write

jobs:
  docker:
    runs-on: ubuntu-latest
    environment: docker-registry  # For Docker Hub secrets
    steps:
      # ... checkout, build ...

      - name: Log in to Docker Hub
        uses: docker/login-action@v3
        with:
          username: ${{ secrets.DOCKERHUB_USERNAME }}
          password: ${{ secrets.DOCKERHUB_TOKEN }}

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: |
            myuser/myimage
            ghcr.io/${{ github.repository }}
          tags: |
            type=raw,value=latest,enable={{is_default_branch}}
            type=semver,pattern={{version}}
            type=sha
```

Both registries share the same tags from one metadata step.

## Key points

- `github.actor` is the user who triggered the workflow
- `GITHUB_TOKEN` is automatically provided — no secret to configure
- Package appears at `github.com/{owner}/{repo}/pkgs/container/{image}`
- First push creates the package automatically
- Package visibility defaults to private; change via package settings
