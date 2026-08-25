#!/bin/bash

SERVER="127.0.0.1"
PORT="5354"

throughput_test() {
    echo "=== Throughput Test (Target: 50k qps) ==="

    local duration=10
    local parallel=100
    local queries_per_worker=5000

    # Install dnsperf if needed: apt install dnsperf
    if command -v dnsperf &> /dev/null; then
        echo "google.com A" > /tmp/queries.txt
        echo "github.com A" >> /tmp/queries.txt
        echo "example.com A" >> /tmp/queries.txt

        dnsperf -s $SERVER -p $PORT -d /tmp/queries.txt -l $duration -c $parallel -Q 800000
    else
        # Fallback: parallel dig
        echo "dnsperf not found, using parallel dig..."

        local start=$(date +%s.%N)
        local total=10000

        seq 1 $total | xargs -P $parallel -I {} dig @$SERVER -p $PORT google.com +short +timeout=1 > /dev/null 2>&1

        local end=$(date +%s.%N)
        local elapsed=$(echo "$end - $start" | bc)
        local qps=$(echo "$total / $elapsed" | bc)

        echo "Completed $total queries in ${elapsed}s"
        echo "Throughput: $qps qps"
    fi
}

latency_test() {
    echo ""
    echo "=== Latency Test (Target: <1ms p99) ==="

    local count=1000
    local results="/tmp/latency_results.txt"

    > $results

    for i in $(seq 1 $count); do
        # Extract query time in ms
        latency=$(dig @$SERVER -p $PORT google.com +timeout=1 2>/dev/null | grep "Query time:" | awk '{print $4}')
        echo $latency >> $results
    done

    # Calculate percentiles
    sort -n $results > /tmp/sorted.txt

    local p50=$(sed -n "$((count/2))p" /tmp/sorted.txt)
    local p95=$(sed -n "$((count*95/100))p" /tmp/sorted.txt)
    local p99=$(sed -n "$((count*99/100))p" /tmp/sorted.txt)
    local avg=$(awk '{sum+=$1} END {printf "%.2f", sum/NR}' /tmp/sorted.txt)

    echo "$count queries completed"
    echo "  Avg: ${avg}ms"
    echo "  P50: ${p50}ms"
    echo "  P95: ${p95}ms"
    echo "  P99: ${p99}ms"

    if (( $(echo "$p99 < 1" | bc -l) )); then
        echo "✓ Sub-millisecond p99 achieved"
    else
        echo "✗ P99 above 1ms"
    fi
}

concurrency_test() {
    echo ""
    echo "=== Concurrency Test ==="

    local concurrent=500
    local domain="google.com"

    echo "Spawning $concurrent concurrent queries..."

    local start=$(date +%s.%N)

    for i in $(seq 1 $concurrent); do
        dig @$SERVER -p $PORT $domain +short +timeout=2 > /dev/null 2>&1 &
    done

    wait

    local end=$(date +%s.%N)
    local elapsed=$(echo "$end - $start" | bc)

    echo "Completed $concurrent concurrent queries in ${elapsed}s"
    echo "Effective rate: $(echo "$concurrent / $elapsed" | bc) qps"
}

run_all() {
    echo "╔════════════════════════════════════════╗"
    echo "║     DNS Analytics Proxy Stress Test    ║"
    echo "║     Server: $SERVER:$PORT               ║"
    echo "╚════════════════════════════════════════╝"
    echo ""

    throughput_test
    latency_test
    cache_test
    blocklist_test
    concurrency_test

    echo ""
    echo "=== All Tests Complete ==="
}

# --- Usage ---
case "${1:-all}" in
    throughput) throughput_test ;;
    latency) latency_test ;;
    cache) cache_test ;;
    blocklist) blocklist_test ;;
    concurrency) concurrency_test ;;
    sustained) sustained_test ;;
    all) run_all ;;
    *)
        echo "Usage: $0 {throughput|latency|cache|blocklist|concurrency|sustained|all}"
        exit 1
        ;;
esac
