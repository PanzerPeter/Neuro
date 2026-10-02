// Scalar floating-point hot loop: Mandelbrot escape counts over a 1000x1000
// grid. Almost pure f64 arithmetic in a tight nested loop, so it measures how
// well locals are kept in registers and how well the inner loop is optimized.

fn mandel(w: i32, h: i32, max_iter: i32) -> i32 {
    let mut total = 0;
    for py in 0..h {
        for px in 0..w {
            let x0 = px as f64 / w as f64 * 3.5 - 2.5;
            let y0 = py as f64 / h as f64 * 2.0 - 1.0;
            let (mut x, mut y, mut i) = (0.0f64, 0.0f64, 0);
            while i < max_iter {
                let (xx, yy) = (x * x, y * y);
                if xx + yy > 4.0 {
                    break;
                }
                let xt = xx - yy + x0;
                y = 2.0 * x * y + y0;
                x = xt;
                i += 1;
            }
            total += i;
        }
    }
    total
}

fn main() {
    println!("mandel = {}", mandel(1000, 1000, 400));
}
