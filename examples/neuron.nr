// Neuro — AI-first compiled language
//
// A single perceptron (neuron) model demonstrating:
//   structs · impl blocks · associated functions · instance methods
//   val/mut bindings · if/block expressions · implicit returns

struct Neuron {
    weight: f64,
    bias: f64
}

impl Neuron {
    func new(weight: f64, bias: f64) -> Neuron {
        Neuron { weight: weight, bias: bias }
    }

    // ReLU activation: pass-through if positive, clamp to zero otherwise
    func activate(&self, input: f64) -> f64 {
        val z = (input * self.weight) + self.bias
        if z > 0.0 { z } else { 0.0 }
    }

    func is_active(&self, input: f64) -> bool {
        val z = (input * self.weight) + self.bias
        z > 0.0
    }
}

func main() -> i32 {
    val neuron = Neuron::new(0.5, -0.1)

    // Dead region: input too small to overcome the bias
    val dead = neuron.activate(0.0)    // 0.0 * 0.5 + (−0.1) = −0.1 → 0.0

    // Active region: strong enough input fires the neuron
    val active = neuron.activate(1.0)  // 1.0 * 0.5 + (−0.1) =  0.4

    // Predicate method using block-expression implicit return
    val fired = neuron.is_active(1.0)

    return 0
}
