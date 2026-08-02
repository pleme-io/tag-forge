# tag-forge — Claude / agent guidance

## What this repo is

`tag-forge` is the architectural-foundation tool of the FinOps forge distribution.
It validates and enforces the cost-allocation tag taxonomy. Every other forge
tool (`attribution-forge`, `unit-econ-forge`, etc.) depends on its output:
without consistent tags, none of the downstream analysis works.

**Open source under MIT.** The framework here is generic; organization-specific
overlays (real tenant lists, cost-center enums, product taxonomies) live in
separate private repos and are versioned independently. **No real
organization's internal naming should land in this repo.** Examples use
fictional names.

Design spec: see Confluence — *FinOps — Forge Distribution: Rust Tools &amp; Claude Skills*.
Strategic context: *FinOps — Strategy, Architecture &amp; Continuous Practice (2026+)*.

## Architecture in one paragraph

The binary parses a YAML config that declares a taxonomy (required dimensions,
optional dimensions, per-dimension validators, virtual-allocation rules). Configs
can `extends:` another file to overlay org-specific deltas onto a generic base.
The `validate` subcommand reads a plan of resources (each with `tags` and / or
`labels`) and reports any missing-required or invalid-value violations. The
`scan` subcommand walks a directory of plan files and aggregates coverage per
dimension.

## Module map

- `src/main.rs` — clap CLI dispatch.
- `src/config.rs` — top-level `Config` struct + `extends:` resolver.
- `src/taxonomy.rs` — the core type: `Taxonomy`, `Validator`, `Violation`. Tests
  per validator type live here. **The validation logic is here, not in `validate.rs`.**
- `src/validate.rs` — the `validate` subcommand: plan parsing + report rendering.
- `src/scan.rs` — the `scan` subcommand: directory walk + coverage aggregation.

## Invariants

1. **Validation is pure** — no I/O inside `Taxonomy::validate_resource`. The
   I/O happens in the subcommand modules.
2. **`extends:` is mergeable** — adding a new field to `Taxonomy` requires
   updating `Taxonomy::merged_with` so overlays compose correctly.
3. **No cloud API calls** in v0.1. Scanning reads local YAML only. Cloud
   integrations (AWS Config, GCP Asset Inventory, Azure Resource Graph) come
   later as separate sub-tools.
4. **The exit code is the contract** — `validate` exits 1 on violation unless
   `--no-fail` is passed. CI/CD gates depend on this.

## When extending

- **New validator type** — add a variant to `Validator` enum, update
  `format_validator`, update `check_validator`, add a test in
  `taxonomy::tests`.
- **New plan format** — write a parser in a new module, convert to `Plan`, feed
  through the existing taxonomy.
- **New output format** — add a branch in `main.rs` that calls a `render_*`
  method on the report.

## Don't

- Don't add cloud SDK dependencies to v0.1 — they belong in companion forge
  tools (`attribution-forge`), not here.
- Don't gate on tag values inside the binary by hardcoding (e.g. "cost_center
  must start with cc-"). All policy lives in the config.
- Don't break the `extends:` merge semantics — overlays must be additive.
- **Don't commit any organization-specific values** to this public repo.
  Tenant codes, real cost-center enum values, real product names, customer
  IDs — none of these belong here. Examples must use fictional names.
