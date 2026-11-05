// `.parse::<T>()`: text read as an integer, a float or a `bool`, answered as
// `Result<T, ParseError>`. The text must be the whole value, with no spaces around it. An
// integer is a decimal with an optional sign, and a float rounds once, in `T` itself.

func describe(text: &string) -> string {
    match text.parse::<u8>() {
        Ok(n) => "{n} fits a u8",
        Err(ParseError::Empty) => "there is no text",
        Err(ParseError::Invalid) => "not a number",
        Err(ParseError::OutOfRange) => "a number, but not a u8"
    }
}

// `?` hands the first failure back to the caller.
func area(width: &string, height: &string) -> Result<f64, ParseError> {
    val w = width.parse::<f64>()?
    val h = height.parse::<f64>()?
    Ok(w * h)
}

func main() -> i32 {
    val samples: [string; 4] = ["200", "", "twelve", "300"]
    for sample in &samples {
        println("'{sample}': " + describe(sample))
    }

    // `??` supplies a default, and the receiver stays usable: `.parse` only reads it.
    val port = "8080"
    val number = port.parse::<i32>() ?? 80
    val fallback = "not a port".parse::<i32>() ?? 80
    println("{port} -> {number}, fallback {fallback}")

    // A view parses without a copy.
    val record = "id=42;"
    val id = record.slice(3..5).parse::<i32>() ?? 0

    val one = "2.5"
    val two = "4"
    val wide = "wide"
    match area(&one, &two) {
        Ok(a) => println("area {a}"),
        Err(e) => println("failed: {e:?}")
    }
    match area(&one, &wide) {
        Ok(a) => println("area {a}"),
        Err(e) => println("failed: {e:?}")
    }

    // Each float type rounds the text directly, so a narrow type is as exact as it can be.
    val narrow = "0.1".parse::<f16>() ?? 0.0f16
    val wider = "0.1".parse::<f64>() ?? 0.0
    println("{narrow as f64:.6} {wider:.6}")

    // Past a float's range the value rounds to infinity, as IEEE 754 says.
    val huge = "1e400".parse::<f64>() ?? 0.0
    val flag = "true".parse::<bool>() ?? false
    println("{huge} {flag}")

    id
}
