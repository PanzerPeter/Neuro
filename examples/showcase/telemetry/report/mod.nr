// Module `report`: a directory module, so `report/format.nr` is its child.

// A re-export: `clamp` is declared in the child, and this line makes it reachable as
// `report::clamp` too, so a caller needs no route to the child at all. A plain `import`
// would bind the name here without opening it to anyone else.
export import ./format::{clamp}


// Exported, so `main` can match on a band through this module's path.
export enum Band {
    Quiet,
    Normal,
    Loud
}

export func band_for(mean: i32) -> Band {
    if mean < 15 {
        Band::Quiet
    } else if mean < 30 {
        Band::Normal
    } else {
        Band::Loud
    }
}

// A trait with a default method. An impl in another module writes `weight` and gets
// `doubled` without writing it.
export trait Weighted {
    func weight(&self) -> i32

    func doubled(&self) -> i32 {
        self.weight() * 2
    }
}

// Absence is a value: a batch that read nothing has no band to weigh. `Some` and `None`
// need no import here either: the prelude binds them in every module, not just the root.
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
