#!/bin/sh
# File is source on `cs` into the project's root. Allows to define a set of project-specific commands and aliases.

#alias r="sc lrun -- start"

alias t="c lrun -- new --tf=5m --size=-0.1 --coin=ADA"
alias tb="c r -- --noconfirm new --tf=5m --size=0.1 --coin=ADA -f='ts:p-0.0002'"
alias ts="c r -- --noconfirm new --tf=5m --size=-0.1 --coin=ADA -f='sar:t5m:s0.07:i0.02:m0.15'"
