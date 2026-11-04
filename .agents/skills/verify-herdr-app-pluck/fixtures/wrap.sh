#!/bin/sh
# Lines longer than any pane: soft-wrapped tokens.
printf 'long url https://example.com/a/very/long/path/that/keeps/going/and/going/well/past/the/edge/of/any/reasonable/pane/width/index.html?query=1\n'
printf 'long path /tmp/aaaaaaaaaa/bbbbbbbbbb/cccccccccc/dddddddddd/eeeeeeeeee/ffffffffff/gggggggggg/hhhhhhhhhh/iiiiiiiiii/jjjjjjjjjj/kkkkkkkkkk/llllllllll/end.txt\n'
printf 'short after 1a63023f\n'
echo end-of-fixture
