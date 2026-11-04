#!/bin/sh
# Heavy SGR: 16/256/truecolor fg and bg, reverse video, bold/italic/underline.
printf '\033[31mred /tmp/red\033[0m \033[1;32mbold green 1a63023f\033[0m \033[3;4mitalic underline\033[0m\n'
printf '\033[38;5;208m256 orange /tmp/orange\033[0m \033[48;5;22mon dark green 10.0.0.1\033[0m\n'
printf '\033[38;2;255;0;255mtruecolor https://example.com/magenta\033[0m \033[48;2;40;40;90m bg truecolor 9876543210 \033[0m\n'
printf '\033[7mreverse video src/rev.rs\033[27m normal \033[7;33mrev yellow\033[0m\n'
printf '\033[38:2::0:200:200mcolon form /tmp/colon\033[0m \033[9mstrike\033[0m \033[2mfaint\033[0m\n'
echo end-of-fixture
