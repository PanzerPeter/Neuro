// Showcase: typed settings read out of raw key/value text, where every step can fail
// and each failure keeps its reason until the code decides it no longer cares.
//
// Integration example combining the `Option` / `Result` methods with the features
// they replace a `match` for:
//   `.ok_or` / `.map_err` / `.map` / `.ok` / `.is_ok` / `.is_err` · `?` propagation ·
//   an enum variant passed as the function `.map_err` takes · `??` with a default, with
//   `continue`, and with `panic` as the unwrap · an enum with payloads + match ·
//   closures and a named function as values · `.chars()` iteration · a borrowed array
//   of structs · string interpolation.
//
// `?` converts nothing, so a lookup's `Option` and a parse's `string` error are both
// lifted into `LoadError` before they propagate. `main` returns the sum of the values
// that loaded, which only comes out right if every default and every skip was taken.

enum LoadError {
    Missing(string),
    Parse(string),
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

// Base-10 digits only: anything else is reported with the character that broke it.
func parse_digits(text: &string) -> Result<i32, string> {
    if text.len() == 0 {
        return Err("empty value")
    }
    mut n = 0
    for c in text.chars() {
        val code = c as u32
        if code < 48 || code > 57 {
            return Err("'{c}' is not a digit")
        }
        n = n * 10 + (code - 48) as i32
    }
    Ok(n)
}

func setting_int(settings: &[Setting; 4], key: string) -> Result<i32, LoadError> {
    val found = lookup(settings, &key)
    val text = found.ok_or(LoadError::Missing(key))?
    val n = parse_digits(&text).map_err(LoadError::Parse)?
    if n > 64 {
        return Err(LoadError::Range(n))
    }
    Ok(n)
}

func explain(e: LoadError) -> string {
    match e {
        LoadError::Missing(key) => "no key '{key}'",
        LoadError::Parse(why) => "unparsable, {why}",
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
