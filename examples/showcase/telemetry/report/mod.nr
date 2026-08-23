// Module `report`: a directory module, so `report/format.nr` is its child.

// A re-export: `clamp` is declared in the child, and this line makes it reachable as
// `report::clamp` too, so a caller needs no route to the child at all. A plain `import`
// would bind the name here without opening it to anyone else.
export import ./format::{clamp}


enum Band {
    Quiet,
    Normal,
    Loud
}

func band_for(mean: i32) -> Band {
    if mean < 15 {
        Band::Quiet
    } else if mean < 30 {
        Band::Normal
    } else {
        Band::Loud
    }
}

// `Band` and `band_for` carry no `export`: they are how this module decides, not part
// of what it offers, so only `weight_of` crosses the module boundary.
//
// Absence is a value: a batch that read nothing has no band to weigh. `Some` and `None`
// need no import here either — the prelude binds them in every module, not just the root.
export func weight_of(mean: i32) -> Option<i32> {
    if mean == 0 {
        return None
    }
    Some(match band_for(mean) {
        Band::Quiet => 1,
        Band::Normal => 2,
        Band::Loud => 3
    })
}
