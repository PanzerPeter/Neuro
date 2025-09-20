fn create_vector() -> Tensor<float, [5]> { return 0; }
fn create_matrix() -> Tensor<int, [2, 3]> { return 0; }

fn main() -> int {
    let v: Tensor<float, [5]> = create_vector();
    let m: Tensor<int, [2, 3]> = create_matrix();
    return 0;
}

