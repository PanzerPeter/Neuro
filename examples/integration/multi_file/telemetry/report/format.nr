// Module `report::format`: reached through its parent directory module, either by
// naming it (`report::format::scale`) or through the `export import` its parent writes.

export func scale(value: i32, factor: i32) -> i32 {
    value * factor
}

export func clamp(value: i32, ceiling: i32) -> i32 {
    if value > ceiling {
        ceiling
    } else {
        value
    }
}
