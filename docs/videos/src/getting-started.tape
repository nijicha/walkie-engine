# Getting started video: a real Claude Code session in a small demo app, no voice.
# Claude Code runs in tmux session "walkie-demo" so its two checkpoints can be
# answered from outside (tmux send-keys) while VHS records. Recording ends when
# the tmux session is detached.
# Render: vhs docs/videos/src/getting-started.tape   (from the repo root)

Output docs/videos/src/getting-started-raw.mp4

Set Shell "bash"
Set FontFamily "CaskaydiaCove Nerd Font Mono"
Set FontSize 18
Set Width 1280
Set Height 720
Set Padding 28
Set Margin 0
Set Framerate 24
Set WindowBar Colorful
Set WindowBarSize 36
Set BorderRadius 10
Set Theme { "name": "Burnt Brick Dark", "black": "#2d2d2d", "red": "#e45a64", "green": "#a3b07a", "yellow": "#d8b36a", "blue": "#88a9d6", "magenta": "#d9714a", "cyan": "#6fbcb0", "white": "#c2b9a6", "brightBlack": "#7c766b", "brightRed": "#e8737c", "brightGreen": "#b5c08f", "brightYellow": "#e2c283", "brightBlue": "#a3bde0", "brightMagenta": "#e08a67", "brightCyan": "#8fcbc1", "brightWhite": "#d7cebb", "background": "#1f1f1f", "foreground": "#d7cebb", "cursor": "#d36b44", "selection": "#3a3836" }
Set TypingSpeed 45ms
Set CursorBlink false

Hide
Type "tmux -f docs/videos/src/tmux.conf attach -t walkie-demo"
Enter
Sleep 2s
Show

Wait+Screen@60m /detached/
