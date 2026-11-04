#!/bin/sh
# Prints a fixture, then idles so the pane keeps showing it.
#   run.sh <name.txt>   cat it        run.sh <name.sh>   run it
# The cursor is hidden so pixel diffs compare text, not a blinking cursor.
printf '\033[?25l'
case "$1" in
    *.txt) cat "$1" ;;
    *) "$1" ;;
esac
exec sleep 1000000
