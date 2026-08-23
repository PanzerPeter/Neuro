// A facade module: it declares nothing of its own and re-exports what `geometry` holds.
//
// `export import` does two things at once — it binds the names here like any import, and
// it makes them reachable *through* this module, so a caller writes `api::Point` without
// ever naming `geometry`. A plain `import` would do only the first half.

export import ./geometry::{Point, ORIGIN_SHIFT as SHIFT}
