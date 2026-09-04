# project-config

## Purpose
Parse and hold project metadata from a `neuro.toml` workspace file — package name, version, authors, build settings, and dependency declarations.

## Entry Point
- Type: Library (no entry function — pure data)
- Key types: `ProjectConfig`, `PackageConfig`, `BuildConfig`, `Dependency`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
None within the workspace — this crate depends only on `serde`/`toml`.

## Notes
Data structures and TOML deserialization only, no compiler business logic.

**This crate has no consumer.** No crate in the workspace declares it as a dependency and no
Rust file outside it names `project_config`, so nothing here reaches a compiled program.
`neurc` takes its input path and options from the command line and never looks for a
`neuro.toml`. The crate reserves the shape a workspace manifest will take once `neurpm`
(Phase 9) needs one.

`ProjectConfig.dependencies` is declared surface with **no resolver behind it**: the field
parses and is then ignored. Nothing fetches, version-solves, or adds a dependency's modules
to the build, so a populated `[dependencies]` table is silently inert rather than an error.
