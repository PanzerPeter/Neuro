# Source for assets/demo.gif — regenerate with:  vhs assets/demo.tape
#
# Requires: vhs, ttyd, ffmpeg, and a release build of the compiler:
#   cargo build --release -p neurc
# Run from the repository root. Every command below is real; the recorded
# output is whatever the compiler actually prints.

Output assets/demo.gif

Require vhs
Require bash

Set Shell bash
Set FontSize 21
Set Width 1360
Set Height 440
Set Padding 28
Set Margin 0
Set WindowBar Colorful
Set WindowBarSize 40
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 40ms

# --- session setup (not recorded) ----------------------------------------
Hide
Type "export LC_ALL=C.UTF-8" Enter
Type "export PATH=$PWD/target/release:$PATH" Enter
Type "export PS1='\[\e[38;5;117m\]neuro\[\e[0m\] \[\e[38;5;212m\]❯\[\e[0m\] '" Enter
Type "clear" Enter
Sleep 500ms
Show

# --- 1. type-check: parse, resolve, type, lower to HIR --------------------
Type "neurc check examples/showcase/perceptron.nr"
Sleep 400ms
Enter
Sleep 1800ms

# --- 2. compile to a native binary through LLVM ---------------------------
Type "neurc compile -O2 examples/showcase/perceptron.nr -o perceptron"
Sleep 400ms
Enter
Sleep 2200ms

# --- 3. run it: forward pass per input, then the summed output ------------
#     the perceptron truncates that sum into the process exit code
Type "./perceptron"
Sleep 400ms
Enter
Sleep 2600ms

# --- cleanup (not recorded) ----------------------------------------------
Hide
Type "rm -f perceptron" Enter
Show
