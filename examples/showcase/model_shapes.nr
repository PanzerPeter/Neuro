// Showcase — a network's layer stack, declared with real tensor parameters.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation —
//   tensor literal coercion + the construction helpers · structs holding
//   tensor fields · an enum matched to pick an initializer · a trait with an
//   `impl` per layer kind · a fixed-size array walked by `for`-in · string
//   interpolation with the format mini-language · move semantics.
//
// Every weight below is a real buffer the binary carries or fills at startup;
// a tensor is not `Copy`, so building a layer MOVES its weights into it. What a
// tensor cannot do yet is be read back — indexing, arithmetic, and reductions are
// later 2B items — so the report is built from the shapes, which are part of the
// types, and the exit code is the total parameter count modulo 256.

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

// A tensor parameter names its full shape, so a caller with any other shape is a
// compile error rather than a runtime one.
func embedding_rows(table: Tensor<f32, [3, 4]>) -> i32 {
    return 3
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

    // Rank-0 tensors have no literal form; they are built directly.
    val scale: Tensor<f32, []> = Tensor::scalar(0.5)

    // Without a tensor annotation this stays a plain `[i32; 3]`, indexable as one.
    val widths = [4, 8, 4]

    mut total = embedding_rows(embeddings) * widths[0]
    println("embeddings [3, 4] via literal   = {total:>4} params")

    val projection_line = projection.describe()
    println("{projection_line:<32}= {projection.parameters():>4} params")
    total = total + projection.parameters()

    val dense_line = dense.describe()
    println("{dense_line:<32}= {dense.parameters():>4} params")
    total = total + dense.parameters()

    for width in widths {
        total = total + width
    }

    println("total parameters                = {total:>4}")
    return total % 256
}
