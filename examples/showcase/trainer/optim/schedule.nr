// Module `optim::schedule`: how the step size shrinks as training goes on.

export func decay(base: f32, step: i32) -> f32 {
    base / (1.0f32 + 0.01f32 * step as f32)
}
