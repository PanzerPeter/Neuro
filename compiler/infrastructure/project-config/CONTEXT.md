# project-config

## Purpose
Parse and hold project metadata from `neuro.toml` workspace configuration files — package name, version, authors, build settings, and dependency declarations.

## Entry Point
- Type: Library (no entry function — pure data)
- Key types: `ProjectConfig`, `PackageConfig`, `BuildConfig`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
No upstream dependencies within the Neuro workspace (uses only `serde`/`toml` from the ecosystem).

## Notes
Pure infrastructure: data structures and TOML deserialization only, no compiler business logic. Read by `neurc` at startup to discover workspace settings. Dependency resolution fields are present as data structures but resolution logic is a Phase 9 feature.
