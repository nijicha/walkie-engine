# Installation video: a real fresh install, no voice.
# Before recording: walkie, the plugin and ~/.cache/walkie are removed (docs/videos/src/reset.sh).
# Render: vhs docs/videos/src/install.tape   (from the repo root)

Output docs/videos/src/install-raw.mp4

Set Shell "bash"
Set FontFamily "CaskaydiaCove Nerd Font Mono"
Set FontSize 20
Set Width 1280
Set Height 720
Set Padding 28
Set Margin 0
Set WindowBar Colorful
Set WindowBarSize 36
Set BorderRadius 10
Set Theme { "name": "Burnt Brick Dark", "black": "#2d2d2d", "red": "#e45a64", "green": "#a3b07a", "yellow": "#d8b36a", "blue": "#88a9d6", "magenta": "#d9714a", "cyan": "#6fbcb0", "white": "#c2b9a6", "brightBlack": "#7c766b", "brightRed": "#e8737c", "brightGreen": "#b5c08f", "brightYellow": "#e2c283", "brightBlue": "#a3bde0", "brightMagenta": "#e08a67", "brightCyan": "#8fcbc1", "brightWhite": "#d7cebb", "background": "#1f1f1f", "foreground": "#d7cebb", "cursor": "#d36b44", "selection": "#3a3836" }
Set TypingSpeed 45ms
Set CursorBlink false

Hide
Type "export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ENV_HINTS=1 PS1='\[\e[38;2;211;107;68m\]❯\[\e[0m\] ' && cd ~ && clear"
Enter
Sleep 1s
Show

Sleep 800ms
Type "# 1. Install the engine"
Sleep 600ms
Enter
Type "brew install nijicha/tap/walkie"
Sleep 400ms
Enter
Wait+Line@15m /❯\s*$/
Sleep 1.5s

Type "walkie setup"
Sleep 400ms
Enter
Wait+Line@15m /❯\s*$/
Sleep 1.5s

Type "clear"
Enter
Type "# 2. Install the plugin (same as /plugin install in Claude Code)"
Sleep 600ms
Enter
Type "claude plugin marketplace add nijicha/skills"
Sleep 400ms
Enter
Wait+Line@3m /❯\s*$/
Sleep 1s
Type "claude plugin install walkie@nijicha-skills"
Sleep 400ms
Enter
Wait+Line@3m /❯\s*$/
Sleep 1.5s

Type "clear"
Enter
Type "# 3. Check that it works"
Sleep 600ms
Enter
Type "walkie version"
Sleep 400ms
Enter
Wait+Line@1m /❯\s*$/
Sleep 1s
Type "claude mcp list | grep walkie"
Sleep 400ms
Enter
Wait+Line@3m /❯\s*$/
Sleep 1s
Type "walkie smoke"
Sleep 400ms
Enter
Wait+Line@10m /❯\s*$/
Sleep 3s
