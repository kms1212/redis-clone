#!/usr/bin/env bash
#
# Compares server builds by CPU time spent per command (user + sys), not requests per second.
# Throughput from redis-benchmark is bound by round-trip latency and, with --threads,
# measured in coarse 250ms steps; CPU time is precise and ignores how fast the client is.
#
#   scripts/cpubench.sh "ECHO hello" old=./old-binary new=target/release/redis-clone
#   ROUNDS=6 COUNT=3000000 scripts/cpubench.sh "ECHO a b c" a=bin-a b=bin-b
#
# Builds run alternately each round so drift (thermal, background load) hits all of them.
# /usr/bin/time reports CPU time in 10ms steps, so keep COUNT large (3M ≈ 0.4% resolution).

set -uo pipefail

ROUNDS="${ROUNDS:-6}"
COUNT="${COUNT:-3000000}"
PORT="${PORT:-6391}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ $# -lt 2 ]; then
  sed -n '3,10p' "$0"
  exit 1
fi
read -r -a command <<< "$1"
shift

results="$(mktemp)"
trap 'rm -f "$results" "$results".time' EXIT

for round in $(seq 1 "$ROUNDS"); do
  for spec in "$@"; do
    name="${spec%%=*}"
    binary="${spec#*=}"
    /usr/bin/time -l "$binary" --port "$PORT" > /dev/null 2> "$results.time" &
    sleep 0.4
    python3 -I "$ROOT/loadgen.py" "$PORT" "$COUNT" "${command[@]}" || exit 1
    pkill -TERM -f "^$binary --port $PORT"
    wait 2> /dev/null
    awk -v round="$round" -v name="$name" -v count="$COUNT" \
      '/ user /{ printf "%s %s %.6f\n", round, name, ($3 + $5) * 1e9 / count }' \
      "$results.time" >> "$results"
    echo "round $round $name done" >&2
  done
done

python3 -I - "$results" "$1" <<'PY'
import collections, statistics as st, sys
runs = collections.defaultdict(dict)
order = []
for line in open(sys.argv[1]):
    rnd, name, ns = line.split()
    if name not in order:
        order.append(name)
    runs[name][int(rnd)] = float(ns)
base = runs[order[0]]
print(f"server CPU ns per command, lower is better ({len(base)} rounds)")
for name in order:
    xs = sorted(runs[name].values())
    q = st.quantiles(xs, n=4) if len(xs) > 1 else [xs[0]] * 3
    line = f"{name:12} median {st.median(xs):7.0f}  IQR {100 * (q[2] - q[0]) / st.median(xs):4.1f}%"
    if name != order[0]:
        diffs = [100 * (runs[name][r] / base[r] - 1) for r in base]
        faster = sum(d < 0 for d in diffs)
        line += f"  vs {order[0]}: {st.median(diffs):+5.1f}% (faster in {faster}/{len(diffs)})"
    print(line)
PY
