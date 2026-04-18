func is_flag_set(flags: i32, mask: i32) -> bool {
    return (flags & mask) != 0
}

func set_flag(flags: i32, mask: i32) -> i32 {
    return flags | mask
}

func toggle_flag(flags: i32, mask: i32) -> i32 {
    return flags ^ mask
}

func main() -> i32 {
    val READ:    i32 = 1 << 0   // 0b001 = 1
    val WRITE:   i32 = 1 << 1   // 0b010 = 2
    val EXECUTE: i32 = 1 << 2   // 0b100 = 4

    mut perms: i32 = 0

    perms = set_flag(perms, READ)
    perms = set_flag(perms, WRITE)

    val has_read:    bool = is_flag_set(perms, READ)
    val has_execute: bool = is_flag_set(perms, EXECUTE)

    perms = toggle_flag(perms, WRITE)

    val full_mask: i32 = READ | WRITE | EXECUTE   // 0b111 = 7
    val active: i32 = perms & full_mask

    return active
}
