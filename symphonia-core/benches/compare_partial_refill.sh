#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
mkdir -p target/criterion
report_dir=$(mktemp -d "$PWD/target/criterion/partial-refill.XXXXXX")
echo "Reports and logs: $report_dir"

export RUSTFLAGS='-C target-cpu=native'
for previous in byte_loop padded_copy; do
    export CRITERION_HOME="$report_dir/$previous"
    mkdir -p "$CRITERION_HOME"
    echo "Measuring $previous baseline (output saved to baseline.log)..."
    if ! BIT_REFILL_IMPL="$previous" cargo bench -p symphonia-core --bench partial_refill -- \
        "$@" --save-baseline "$previous" > "$CRITERION_HOME/baseline.log" 2>&1; then
        cat "$CRITERION_HOME/baseline.log" >&2
        exit 1
    fi

    echo "Current vs $previous (negative time change means faster):"
    if ! BIT_REFILL_IMPL=current cargo bench -p symphonia-core --bench partial_refill -- \
        "$@" --baseline "$previous" 2> "$CRITERION_HOME/progress.log" | \
        tee "$CRITERION_HOME/comparison.log"; then
        cat "$CRITERION_HOME/progress.log" >&2
        exit 1
    fi
done
