#!/bin/sh
# CJK, emoji (ZWJ, VS16, flags, skin tones), combining marks and tabs, each followed by tokens.
printf 'cjk     日本語のパス src/日本/main.rs then 1a63023f\n'
printf 'emoji   🔥 /tmp/fire 👨‍👩‍👧 /tmp/family ❤️ /tmp/heart 🇯🇵 /tmp/flag 👍🏽 /tmp/thumb\n'
printf 'combine e\314\201 /tmp/accent cafe\314\201/menu.txt 192.168.40.12\n'
printf 'tabs\t/tmp/tab1\t\t/tmp/tab2\tdeadbee1\n'
printf 'zwsp    a\342\200\213b /tmp/zwsp https://example.com/zw\n'
echo end-of-fixture
