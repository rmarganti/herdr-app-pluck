#!/bin/sh
# OSC 8 named links: visible text differs from the target.
printf 'named \033]8;;https://github.com/rmarganti\033\\GitHub\033]8;;\033\\ link\n'
printf 'file  \033]8;;file:///tmp/report.html\033\\the report\033]8;;\033\\ and a plain /tmp/plain\n'
echo end-of-fixture
