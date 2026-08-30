// Scalar floating-point hot loop: Mandelbrot escape counts over a 1000x1000
// grid. Almost pure f64 arithmetic in a tight nested loop, so it measures how
// well locals are kept in registers and how well the inner loop is optimized.

func mandel(w: i32, h: i32, max_iter: i32) -> i32 {
    mut total: i32 = 0
    mut py: i32 = 0
    while py < h {
        mut px: i32 = 0
        while px < w {
            val x0 = (px as f64) / (w as f64) * 3.5 - 2.5
            val y0 = (py as f64) / (h as f64) * 2.0 - 1.0
            mut x: f64 = 0.0
            mut y: f64 = 0.0
            mut i: i32 = 0
            while i < max_iter {
                val xx = x * x
                val yy = y * y
                if xx + yy > 4.0 {
                    break
                }
                val xt = xx - yy + x0
                y = 2.0 * x * y + y0
                x = xt
                i += 1
            }
            total += i
            px += 1
        }
        py += 1
    }
    return total
}

func main() -> i32 {
    val r = mandel(1000, 1000, 400)
    println("mandel = {r}")
    return 0
}
