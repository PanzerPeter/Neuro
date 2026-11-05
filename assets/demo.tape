# Source for assets/demo.gif. Regenerate with:  vhs assets/demo.tape
#
# Requires: vhs, ttyd, ffmpeg, bat, an NVIDIA GPU, and a build of the compiler:
#   cargo build -p neurc
# Run from the repository root. Every command below is real; the recorded
# output is whatever the compiler actually prints.
#
# The program is examples/showcase/train_predict.nr with its comments removed.
# Setup swaps one matrix product, so the demo has a real shape error to catch.

Output assets/demo.gif

Require vhs
Require bash
Require bat

Set Shell bash
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 19
Set Width 1336
Set Height 510
Set Padding 28
Set Margin 0
Set WindowBar Colorful
Set WindowBarSize 40
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set TypingSpeed 28ms

# --- session setup (not recorded) ----------------------------------------
Hide
Type "export LC_ALL=C.UTF-8 BAT_STYLE=plain BAT_PAGING=never" Enter
Type "export PATH=$PWD/target/debug:$PATH" Enter
Type "export PS1='\[\e[38;5;117m\]neuro\[\e[0m\] \[\e[38;5;212m\]❯\[\e[0m\] '" Enter
Type "demo=$(mktemp -d)" Enter
Type "grep -v '^ *//' examples/showcase/train_predict.nr | sed '/./,$!d; s/x @ w - y/w @ x - y/' > $demo/fit.nr" Enter
Type "cd $demo" Enter
Type "clear" Enter
Sleep 500ms
Show

# --- 1. the program: a loss with gradients, and a GPU function ------------
Type "# A training loss and a GPU function. Spot the bug?"
Enter
Sleep 300ms
Type "bat -l rust -r :12 fit.nr"
Sleep 300ms
Enter
Sleep 4500ms
Ctrl+L

# --- 2. the compiler catches the swapped product before anything runs -----
Type "# Tensor shapes are part of the type, so this never reaches a run."
Enter
Sleep 300ms
Type "neurc check fit.nr"
Sleep 300ms
Enter
Sleep 5000ms
Ctrl+L

# --- 3. fix it, build a native binary, train, predict on the GPU ----------
Type "# Fix it, build one small native binary, and run it."
Enter
Sleep 300ms
Type "sed -i 's/w @ x/x @ w/' fit.nr"
Enter
Sleep 400ms
Type "neurc compile -O3 fit.nr -o fit"
Enter
Sleep 900ms
Type "ls -sh fit"
Enter
Sleep 900ms
Type "./fit"
Sleep 300ms
Enter
Sleep 5000ms

# --- cleanup (not recorded) ----------------------------------------------
Hide
Type "cd - && rm -rf $demo" Enter
Show
