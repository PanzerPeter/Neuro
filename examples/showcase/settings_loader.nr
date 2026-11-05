// Showcase: typed settings read from raw key/value text, where each step can fail and keeps
// its reason until the code no longer needs it. Integration example: `Option` and `Result`
// methods (`.ok_or`, `.map_err`, `.map`, `.ok`, `.is_ok`, `.is_err`), `.parse::<T>()`, `?`, an
// enum variant passed as a function, `??` with a default, `continue` or `panic`, and closures
// and named functions as values. `?` converts nothing, so a missing key's `Option` and a
// parse's `ParseError` are both lifted into `LoadError` before they propagate. The exit code
// is the sum of the values that loaded.

enum LoadError {
    Missing(string),
    Parse(ParseError),
    Range(i32)
}

struct Setting {
    key: string,
    value: string
}

func lookup(settings: &[Setting; 4], key: &string) -> Option<string> {
    for setting in settings {
        if setting.key == key {
            return Some(setting.value.clone())
        }
    }
    None
}

func setting_int(settings: &[Setting; 4], key: string) -> Result<i32, LoadError> {
    val found = lookup(settings, &key)
    val text = found.ok_or(LoadError::Missing(key))?
    val n = text.parse::<i32>().map_err(LoadError::Parse)?
    if n > 64 {
        return Err(LoadError::Range(n))
    }
    Ok(n)
}

func explain(e: LoadError) -> string {
    match e {
        LoadError::Missing(key) => "no key '{key}'",
        LoadError::Parse(why) => "unparsable: {why:?}",
        LoadError::Range(n) => "{n} is above 64"
    }
}

func render(r: Result<i32, LoadError>) -> string {
    match r.map(|n| "= {n}").map_err(explain) {
        Ok(text) => text,
        Err(text) => "! " + text
    }
}

func main() -> i32 {
    val settings: [Setting; 4] = [
        Setting { key: "workers", value: "8" },
        Setting { key: "batch", value: "3x" },
        Setting { key: "retries", value: "900" },
        Setting { key: "seed", value: "42" }
    ]
    val keys: [string; 5] = ["workers", "batch", "depth", "retries", "seed"]

    mut failed = 0
    for i in 0..5 {
        val key = keys[i].clone()
        val label = "{key:<8}"
        val result = setting_int(&settings, key)
        if result.is_err() {
            failed = failed + 1
        }
        println(label + render(result))
    }

    // Each line below decides how much the reason matters.
    val workers = setting_int(&settings, "workers") ?? panic("workers is required")
    val batch = setting_int(&settings, "batch").ok().map(|b| b * 2) ?? 16
    val depth = setting_int(&settings, "depth") ?? 3
    val seeded = setting_int(&settings, "seed").is_ok()

    mut loaded = 0
    for i in 0..5 {
        loaded = loaded + (setting_int(&settings, keys[i].clone()) ?? continue)
    }

    println("workers {workers}, batch {batch}, depth {depth}, seeded {seeded}")
    println("{failed} of 5 settings failed, the rest sum to {loaded}")
    workers + batch + depth + loaded
}
