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

// Absence is a value: a batch that read nothing has no band to weigh.
func weight_of(mean: i32) -> Option<i32> {
    if mean == 0 {
        return Option::None
    }
    Option::Some(match band_for(mean) {
        Band::Quiet => 1,
        Band::Normal => 2,
        Band::Loud => 3
    })
}
