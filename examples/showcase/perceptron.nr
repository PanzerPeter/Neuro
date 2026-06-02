// Showcase — a two-neuron feed-forward pass.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation —
//   structs · impl blocks (associated fn + &self methods) · f64 arithmetic
//   · if/else as a value expression · while loop · `as` cast.
//
// A "hidden" neuron feeds an "output" neuron; a small ramp of inputs is run
// through both and the fired outputs are summed. The truncated sum is returned
// as the process exit code.

struct Neuron {
    weight: f64,
    bias: f64
}

impl Neuron {
    func new(weight: f64, bias: f64) -> Neuron {
        Neuron { weight: weight, bias: bias }
    }

    // Weighted pre-activation: w * x + b.
    func preact(&self, input: f64) -> f64 {
        (input * self.weight) + self.bias
    }

    // ReLU activation.
    func activate(&self, input: f64) -> f64 {
        val z = self.preact(input)
        if z > 0.0 { return z }
        return 0.0
    }
}

func main() -> i32 {
    val hidden = Neuron::new(0.5, -0.2)
    val output = Neuron::new(2.0, 0.1)

    // Feed inputs 0.0, 1.0, ... 4.0 through hidden -> output, summing outputs.
    mut total: f64 = 0.0
    mut x: f64 = 0.0
    while x < 5.0 {
        val h = hidden.activate(x)
        val y = output.activate(h)
        total = total + y
        x = x + 1.0
    }

    // total == 8.9; truncating toward zero yields 8.
    return total as i32
}
