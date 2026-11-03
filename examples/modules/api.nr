// A facade module: it declares nothing of its own and re-exports what `geometry` holds.
// `export import` binds the names here and makes them reachable through `api`, so a caller
// writes `api::Point` without naming `geometry`. A plain `import` does only the first half.

export import ./geometry::{Point, ORIGIN_SHIFT as SHIFT}
