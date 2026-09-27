// Showcase: sensor readings quantized to one byte each, with the out-of-range ones
// caught instead of clamped.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   `.to_checked::<u8>()`, which answers `None` where `as` would saturate ·
//   `match` over the `Option` it returns, and `??` for a default · a closure
//   capturing the scale · a `@derive(Copy)` struct per reading collected in a
//   `Vec` · a range walked with `.step(n)` · interpolation with format specs.
//
// A reading's code is twice its value, truncated toward zero, so a byte holds readings
// from just above -0.5 to just below 128. Past either end `as u8` would clamp to 0 or
// 255, both plausible codes. The last sample is NaN (a sensor that returned nothing),
// which `as u8` turns into 0 and `.to_checked` refuses like any other value a byte
// cannot hold.

const SCALE: f64 = 2.0

@derive(Copy, Clone)
struct Sample {
    index: i32,
    raw: f64,
    code: u8,
    valid: bool
}

func main() -> i32 {
    val readings: [f64; 8] = [12.3, 131.0, 127.4, 127.9, -0.2, -3.5, 99.99, 0.0 / 0.0]
    val quantize = |x: f64| -> Option<u8> { (x * SCALE).to_checked::<u8>() }

    mut samples: Vec<Sample> = Vec::new()
    for i in 0..8 {
        val raw = readings[i]
        val sample = match quantize(raw) {
            Option::Some(code) => Sample { index: i, raw: raw, code: code, valid: true },
            Option::None => Sample { index: i, raw: raw, code: 0u8, valid: false },
        }
        samples.push(sample)
    }

    mut rejected = 0
    mut checksum = 0
    for i in 0..8 {
        val s = samples[i]
        if s.valid {
            println("sample {s.index}: {s.raw:>7.2} -> {s.code:>3}")
            checksum += s.code as i32
        } else {
            println("sample {s.index}: {s.raw:>7.2} -> rejected")
            rejected += 1
        }
    }

    // Every other reading, with a rejected one falling back to the midpoint code.
    mut thinned = 0
    for i in (0..8).step(2) {
        thinned += (quantize(readings[i]) ?? 128u8) as i32
    }

    println("rejected {rejected} of 8, checksum {checksum}, thinned {thinned}")
    return rejected
}
