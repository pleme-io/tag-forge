# tag-forge

Universal cost-allocation tag taxonomy validator and enforcer — **open source under MIT**.

`tag-forge` is the foundation of the FinOps forge distribution — a small, fast
Rust CLI that validates a resource plan against a configured taxonomy of
required and optional dimensions (`cost_center`, `tenant`, `product`,
`environment`, `owner`, etc.), scans directories of manifests for coverage
gaps, and renders the live taxonomy for human review.

It is **generic by construction**. The same binary runs against any org. Every
organization-specific decision — which dimensions are required, which enum
values are valid, which untaggable spend gets virtual-allocated and how — sits
in a config overlay file that inherits the generic reference via an `extends:`
directive.

## Public framework / private config boundary

This is the central architectural decision and worth saying explicitly:

| What | Where | Why |
| --- | --- | --- |
| **The binary** (`tag-forge`, all `*-forge` siblings) | pleme-io — public, MIT | Generic FinOps framework, applies to any org |
| **The reference taxonomy** (`configs/default.yaml`) | pleme-io — public | Sensible defaults; OSS adopters get a working start |
| **The illustrative overlay** (`examples/multi-tenant-saas-overlay.yaml`) | pleme-io — public | Pattern documentation; uses fictional names |
| **Generic skills** (e.g. `finops/SKILL.md`) | pleme-io/blackmatter-claude — public | Workflow guidance that applies to any consumer |
| **Org-specific overlays** (real tenants, real cost centers, real product enums) | private org repo (e.g. `your-org/finops-config`) | Embeds business-internal names and customer-shaped values |

The boundary is clean: every line of code, schema, and reference doc is open
source. Every line of config that names something internal to an organization
lives in that organization's private repo. Different consumers of the
distribution each maintain their own private overlay; the public framework
serves all of them identically.

## Status

Early. Reads YAML plans, applies a configurable taxonomy, reports violations.
No cloud-API integration yet — that lands in the scanner over time.

## Install

```bash
# from source
cargo build --release
./target/release/tag-forge --help

# via Nix
nix run github:pleme-io/tag-forge -- --help
```

## Usage

### Validate a plan

```bash
tag-forge --config configs/default.yaml \
  validate examples/good-resources.yaml
```

Exits non-zero on any violation. Pass `--no-fail` to keep zero exit while
still rendering the report.

### Scan a directory

```bash
tag-forge --config configs/default.yaml scan ./manifests
```

Walks the tree, reads every `*.yaml` / `*.yml` that has a top-level
`resources:` array, and reports per-dimension coverage.

### Inspect the taxonomy

```bash
tag-forge --config configs/default.yaml taxonomy print
tag-forge --config configs/default.yaml taxonomy describe cost_center
```

## Config

The config file declares the taxonomy. Two-layer pattern:

- **Reference config** (`configs/default.yaml` in this repo) — generic; ships
  with the binary.
- **Overlay config** — org-specific. Inherits via `extends:`, only specifies
  deltas. Lives in the org's private config repo.

See `examples/multi-tenant-saas-overlay.yaml` for an illustrative pattern (uses
fictional tenant / cost-center / product names — not a real org).

## Plan format

A plan is a YAML document with a top-level `resources` list. Each resource has
an `id`, optional `kind`, plus a `tags` map (cloud tags) and a `labels` map
(K8s labels). Labels take precedence over tags where both define the same key
— the K8s label is more specific than the underlying node tag.

```yaml
resources:
  - id: aws/ec2/i-abc123
    kind: aws_instance
    tags:
      cost_center: cc-platform
      environment: prod
  - id: k8s/pod/auth-7d9
    kind: kubernetes_pod
    tags:
      cost_center: cc-node-default
    labels:
      cost_center: cc-api      # wins
```

## Contributing

Public framework — issues + PRs welcome. Substantial design changes belong in
an issue first; new validator types, new scan sources, new output formats are
all good directions. Don't add cloud SDK dependencies to this crate — they
belong in companion forge tools (`attribution-forge`, etc.).

## License

MIT — see `LICENSE`.
