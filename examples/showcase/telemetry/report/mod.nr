// Module `report`: a directory module, so `report/format.nr` is its child.

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
// Absence is a value: a batch that read nothing has no band to weigh.
export func weight_of(mean: i32) -> Option<i32> {
    if mean == 0 {
        return Option::None
    }
    Option::Some(match band_for(mean) {
        Band::Quiet => 1,
        Band::Normal => 2,
        Band::Loud => 3
    })
}
