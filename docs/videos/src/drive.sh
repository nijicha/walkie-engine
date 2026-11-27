#!/bin/sh
# Helpers to drive the getting-started session from outside.
#   drive.sh new            start tmux session walkie-demo in ~/walkie-demo
#   drive.sh type "text"    type text like a person (one key at a time)
#   drive.sh key Enter      send one key
#   drive.sh show           print the screen
#   drive.sh end            detach the recording client (ends the video)
here=$(cd "$(dirname "$0")" && pwd)
S=walkie-demo
case "$1" in
  new)
    tmux -f "$here/tmux.conf" kill-session -t $S 2>/dev/null
    tmux -f "$here/tmux.conf" new-session -d -s $S -x 120 -y 30 -c "$HOME/walkie-demo" \
      "env BASH_SILENCE_DEPRECATION_WARNING=1 PS1='\[\e[38;2;211;107;68m\]❯\[\e[0m\] ' bash --norc --noprofile" ;;
  type)
    printf '%s\n' "$2" | fold -w1 | while IFS= read -r c; do
      if [ -z "$c" ]; then tmux send-keys -t $S Space; else tmux send-keys -t $S -l "$c"; fi
      sleep 0.045
    done ;;
  key) tmux send-keys -t $S "$2" ;;
  show) tmux capture-pane -p -t $S ;;
  end) tmux detach-client -s $S ;;
esac
