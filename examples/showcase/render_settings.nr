// Showcase — sub-phase 1H end to end: a render pipeline configured by name.
//
// Integration example combining every 1H language-cleanup feature with the
// machinery that gives them something to say:
//   named arguments with external labels (`quality q:`) and a positional-only
//   `_ ` parameter · string interpolation with the format mini-language
//   (:.N, :>W) · a triple-quoted `"""` block with dedent · nested block
//   comments · struct + impl (&self and &mut self) methods · enum with a
//   payload + match · Option from the implicit prelude · fixed-size array +
//   for-in loop.
//
// Every named call below is written with its arguments in an order that differs
// from the declaration, so the program only produces the right answer if the
// labels — not the positions — decided the binding.

enum Quality {
    Draft,
    Final(i32)
}

@derive(Copy, Clone)
struct Frame {
    width: i32,
    height: i32
}

impl Frame {
    // The caller writes `width:` and `height:`; both are ordinary parameters,
    // so they could also have been passed positionally.
    func make(width: i32, height: i32) -> Frame {
        return Frame { width: width, height: height }
    }

    // `_ factor` is positional-only: `scale(2)` reads better than
    // `scale(factor: 2)`. `min` and `cap` must be named at every call site.
    func scale(&mut self, _ factor: i32, min floor: i32, max cap: i32) {
        mut w: i32 = self.width * factor
        mut h: i32 = self.height * factor
        if w < floor { w = floor }
        if h < floor { h = floor }
        if w > cap { w = cap }
        if h > cap { h = cap }
        self.width = w
        self.height = h
    }

    func pixels(&self) -> i32 {
        return self.width * self.height
    }

    func label(&self) -> string {
        return "{self.width:>4}x{self.height:>4}"
    }
}

/* The budget rule is deliberately asymmetric, so a wrongly ordered call is
   visible in the answer.
   /* A Draft frame is charged at full price; a Final frame is discounted by
      its payload. This inner comment closes without ending the outer one —
      nesting block comments is itself a 1H feature. */
   Everything up to the final delimiter is still part of this comment. */
func budget(quality q: Quality, per_pixel rate: i32, frame f: Frame) -> i32 {
    val raw = f.pixels() * rate
    match q {
        Quality::Draft => raw,
        Quality::Final(discount) => raw - discount
    }
}

// A `"""` block dedents against its closing delimiter's column, so this header
// carries no leading spaces even though it is written indented here.
func header() -> string {
    return """
        render report
        -------------
        """
}

// The first frame over the pixel budget, or None when every frame fits.
func first_over(frames: [Frame; 3], limit: i32) -> Option<i32> {
    mut index: i32 = 0
    for frame in frames {
        if frame.pixels() > limit {
            return Option::Some(index)
        }
        index += 1
    }
    return Option::None
}

func check(actual: string, expected: string) -> i32 {
    if actual == expected {
        return 1
    }
    return 0
}

func main() -> i32 {
    // Named arguments in reverse declaration order.
    mut hero = Frame::make(height: 4, width: 6)
    mut thumb = Frame::make(height: 2, width: 3)
    val fixed = Frame::make(width: 10, height: 10)

    // `2` is positional (its parameter is `_ factor`); the bounds are named,
    // and named out of order.
    hero.scale(2, max: 40, min: 4)
    thumb.scale(5, max: 40, min: 12)

    mut matched: i32 = 0
    val hero_label = hero.label()
    val thumb_label = thumb.label()
    println("hero.label()  = [{hero_label}]")
    println("thumb.label() = [{thumb_label}]")
    matched = matched + check(hero_label, "  12x   8")
    matched = matched + check(thumb_label, "  15x  12")

    val report = header() + "frames checked: {matched}"
    println(report)
    matched = matched + check(report, "render report\n-------------\nframes checked: 2")

    // The same three arguments, named in three different orders.
    val a = budget(frame: hero, per_pixel: 2, quality: Quality::Draft)
    val b = budget(per_pixel: 2, quality: Quality::Final(6), frame: hero)
    val c = budget(quality: Quality::Draft, frame: thumb, per_pixel: 1)

    // hero is 12x8 = 96 px: draft 192, final 192-6 = 186. thumb is 15x12 = 180.
    println("budget draft, hero  = {a}")
    println("budget final, hero  = {b}")
    println("budget draft, thumb = {c}")

    if a != 192 { return 1 }
    if b != 186 { return 2 }
    if c != 180 { return 3 }

    val frames: [Frame; 3] = [thumb, fixed, hero]
    val over = first_over(frames, 120)
    val over_index = over ?? 9

    // thumb is 180 px, so the very first frame is already over the 120 budget.
    if over_index != 0 { return 4 }
    if matched != 3 { return 5 }

    val summary = "{matched} checks, {over_index} first over"
    println(summary)
    if summary != "3 checks, 0 first over" { return 6 }

    // 96 pixels of hero, plus one for each of the three checks that matched.
    val total = hero.pixels() + matched
    println("hero pixels + checks = {total}")
    return total
}
