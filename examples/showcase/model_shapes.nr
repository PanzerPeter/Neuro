// Showcase: a network's layer stack, declared with real tensor parameters.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   tensor literal coercion + the construction helpers · structs holding
//   tensor fields · an enum matched to pick an initializer · a trait with an
//   `impl` per layer kind · a fixed-size array walked by `for`-in · string
//   interpolation with the format mini-language · tensor ownership: moves,
//   `.clone()`, and the `.to(device)` transfer · a dynamic `?` axis reading two
//   shapes through one signature · a weight matrix at real scale, built in one
//   function and returned by value.
//
// Every weight below is a real buffer the binary carries or fills at startup;
// a tensor is not `Copy`, so building a layer MOVES its weights into it, and a
// second owner has to be asked for explicitly with `.clone()`. A tensor value is a
// DLPack handle whose `data` field addresses that buffer out of line, which is why
// `hidden_weights()` below can hand back 100352 parameters by value at the default
// `-O 0`, and why a foreign consumer would take the same pointer unchanged. The report
// here is built from the shapes, which are part of the types, and the exit code is the
// total parameter count modulo 256; reading elements back out is
// `types/tensor_indexing.nr`, and the in-place updates are in `optimizer_step.nr`.

enum Init {
    Zeros,
    Identity,
    Normal
}

trait Described {
    func describe(&self) -> string
    func parameters(&self) -> i32
}

// A 4x4 projection: the one square shape `identity()` applies to.
struct Projection {
    weights: Tensor<f32, [4, 4]>,
    init: Init
}

impl Described for Projection {
    func describe(&self) -> string {
        val kind = match self.init {
            Init::Zeros => "zeros",
            Init::Identity => "identity",
            Init::Normal => "normal"
        }
        return "projection [4, 4] via " + kind
    }

    func parameters(&self) -> i32 {
        return 16
    }
}

// A dense layer: a random-normal weight matrix and a zeroed bias vector.
struct Dense {
    weights: Tensor<f32, [8, 4]>,
    bias: Tensor<f32, [8]>
}

impl Described for Dense {
    func describe(&self) -> string {
        return "dense      [8, 4] + bias [8]"
    }

    func parameters(&self) -> i32 {
        return 40
    }
}

impl Projection {
    // The enum picks which helper builds the buffer. Each arm yields a
    // `Tensor<f32, [4, 4]>`, so the `match` is an expression of that one type.
    func new(init: Init) -> Projection {
        val weights = match init {
            Init::Zeros => Tensor::<f32, [4, 4]>::zeros(),
            Init::Identity => Tensor::<f32, [4, 4]>::identity(),
            Init::Normal => Tensor::<f32, [4, 4]>::random_normal(mean: 0.0f32, std: 0.05f32)
        }
        // `weights` moves into the struct; it is not usable afterwards.
        return Projection { weights: weights, init: init }
    }
}

impl Dense {
    func new() -> Dense {
        return Dense {
            weights: Tensor::<f32, [8, 4]>::random_normal(mean: 0.0f32, std: 0.02f32),
            bias: Tensor::<f32, [8]>::zeros()
        }
    }
}

// A hidden layer at the scale a real model uses: 784x128 weights and a 128-wide
// bias, 100480 parameters between them.
struct Hidden {
    weights: Tensor<f32, [784, 128]>,
    bias: Tensor<f32, [128]>
}

impl Described for Hidden {
    func describe(&self) -> string {
        return "hidden  [784, 128] + bias [128]"
    }

    func parameters(&self) -> i32 {
        return 100480
    }
}

// The weight matrix is built here and RETURNED by value. What crosses the call
// boundary is the tensor's DLPack handle (one pointer) rather than 392 KB of LLVM
// value, which is the reason this compiles at `-O 0`.
func hidden_weights() -> Tensor<f32, [784, 128]> {
    return Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
}

impl Hidden {
    func new() -> Hidden {
        return Hidden { weights: hidden_weights(), bias: Tensor::<f32, [128]>::zeros() }
    }
}

// A `?` axis (dynamic shapes) is the one way out of naming the full shape: this takes a
// table of any height as long as every row is 4 wide, so one function serves the `[3, 4]`
// embedding table and a `[5, 4]` one alike. The axes beside the `?` are checked exactly
// as before, and the `[784, 128]` hidden weights would be rejected on their width.
func row_width(table: &Tensor<f32, [?, 4]>) -> i32 {
    return 4
}

// A tensor parameter names its full shape, so a caller with any other shape is a
// compile error rather than a runtime one. Taking the table by value MOVES it.
func embedding_rows(table: Tensor<f32, [3, 4]>) -> i32 {
    return 3
}

// The zero-cost sharing path: a borrow reads a tensor without consuming it, so the
// caller still owns its table afterwards.
func embedding_width(table: &Tensor<f32, [3, 4]>) -> i32 {
    return 4
}

func main() -> i32 {
    // Literal coercion: the annotation makes this a tensor and types its leaves as
    // `f32`. Every row is 4 wide because the shape says so.
    val embeddings: Tensor<f32, [3, 4]> = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0]
    ]

    val projection = Projection::new(Init::Identity)
    val dense = Dense::new()
    val hidden = Hidden::new()

    // Rank-0 tensors have no literal form; they are built directly.
    val scale: Tensor<f32, []> = Tensor::scalar(0.5)

    // Without a tensor annotation this stays a plain `[i32; 3]`, indexable as one.
    val widths = [4, 8, 4]

    // A tensor is not `Copy`, and `embedding_rows` below takes one by value, so a
    // second owner has to be asked for. `.clone()` is that request, and
    // `.to(Device::CPU)` consumes the copy in turn, handing back the same tensor on
    // the host, the only device this compiler can lower to today.
    val on_host = embeddings.clone().to(Device::CPU)
    mut total = embedding_rows(embeddings) * embedding_width(&on_host)
    println("embeddings [3, 4] via literal   = {total:>6} params")

    val projection_line = projection.describe()
    println("{projection_line:<32}= {projection.parameters():>6} params")
    total = total + projection.parameters()

    val dense_line = dense.describe()
    println("{dense_line:<32}= {dense.parameters():>6} params")
    total = total + dense.parameters()

    val hidden_line = hidden.describe()
    println("{hidden_line:<32}= {hidden.parameters():>6} params")
    total = total + hidden.parameters()

    for width in widths {
        total = total + width
    }

    // `on_host` is still ours: the borrow above read it without taking it.
    total = total + embedding_width(&on_host)

    // Two shapes that share only their second axis, read by one signature. Neither
    // borrow moves anything, and neither call needed the height written down.
    val wide = Tensor::<f32, [5, 4]>::zeros()
    val row_widths = row_width(&on_host) + row_width(&wide)
    println("row_width over [3,4] and [5,4]  = {row_widths:>6} wide")
    total = total + row_widths

    println("total parameters                = {total:>6}")
    return total % 256
}
