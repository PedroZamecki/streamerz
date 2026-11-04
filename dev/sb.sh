#!/bin/bash
# Stop or start the real Streamer.bot (~/Games/streamerbot) under the system Wine: `sb.sh stop`, `sb.sh start`.
# (Lutris's Proton crashes it on start, a WPF UI Automation method missing; plain Wine runs it.)
# Don't run `pkill -f` with these patterns from a shell whose own command line contains them.
SB=$HOME/Games/streamerbot
export WINEPREFIX=$SB/prefix WINEDEBUG=-all
if [ "$1" = stop ]; then
  pkill -f 'Streamer[.]bot[.]exe'
  for i in $(seq 1 30); do pgrep -f 'Streamer[.]bot[.]exe' >/dev/null || break; sleep 1; done
  pkill -9 -f 'Streamer[.]bot[.]exe'
  wineserver -k 2>/dev/null
  sleep 2
else
  (cd "$SB" && env -u VIRTUAL_ENV -u PYTHONHOME -u PYTHONPATH setsid wine ./Streamer.bot.exe >/tmp/sb-launch.log 2>&1 &)
  for i in $(seq 1 90); do sleep 2; ss -ltn | grep -q ':8080 ' && break; done
  sleep 4
fi
