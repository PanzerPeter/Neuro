fn test() -> bool {
    if true || false {
        return true;
    }
    return false;
}

fn main() -> int {
    let result = test();
    return 0;
}