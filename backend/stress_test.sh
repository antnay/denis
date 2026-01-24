#!/bin/bash
# stress_test.sh - Prove resume claims

SERVER="127.0.0.1"
PORT="5354"

# --- 50k+ queries/second test ---
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
        
        dnsperf -s $SERVER -p $PORT -d /tmp/queries.txt -l $duration -c $parallel -Q 60000
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

# --- Sub-millisecond p99 latency test ---
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

# --- Cache hit rate test ---
cache_test() {
    echo ""
    echo "=== Cache Hit Rate Test (Target: 85%) ==="
    
    local domains=("google.com" "github.com" "example.com" "reddit.com" "amazon.com")
    local queries=1000
    local cache_hits=0
    
    # Prime the cache
    for domain in "${domains[@]}"; do
        dig @$SERVER -p $PORT $domain +short > /dev/null 2>&1
    done
    
    sleep 1
    
    # Run queries and check latency (cache hit = fast)
    for i in $(seq 1 $queries); do
        domain=${domains[$((i % ${#domains[@]}))]}
        latency=$(dig @$SERVER -p $PORT $domain +timeout=1 2>/dev/null | grep "Query time:" | awk '{print $4}')
        
        # Assume <5ms is cache hit
        if [[ $latency -lt 5 ]]; then
            ((cache_hits++))
        fi
    done
    
    local hit_rate=$(echo "scale=2; $cache_hits * 100 / $queries" | bc)
    
    echo "$queries queries, $cache_hits cache hits"
    echo "Cache hit rate: ${hit_rate}%"
    
    if (( $(echo "$hit_rate >= 85" | bc -l) )); then
        echo "✓ 85% cache hit rate achieved"
    else
        echo "✗ Cache hit rate below 85%"
    fi
}

# --- Blocklist effectiveness test ---
blocklist_test() {
    echo ""
    echo "=== Blocklist Test ==="
    
    local blocked_domains=(
        "ads.google.com"
        "doubleclick.net"
        "tracking.facebook.com"
        "analytics.google.com"
        "ad.doubleclick.net"
    )
    
    local allowed_domains=(
        "google.com"
        "facebook.com"
        "github.com"
    )
    
    local blocked_count=0
    local allowed_count=0
    
    echo "Testing blocked domains:"
    for domain in "${blocked_domains[@]}"; do
        status=$(dig @$SERVER -p $PORT $domain +timeout=2 2>/dev/null | grep "status:" | awk '{print $6}' | tr -d ',')
        if [[ "$status" == "NXDOMAIN" ]]; then
            echo "  ✓ $domain → NXDOMAIN"
            ((blocked_count++))
        else
            echo "  ✗ $domain → $status (expected NXDOMAIN)"
        fi
    done
    
    echo "Testing allowed domains:"
    for domain in "${allowed_domains[@]}"; do
        status=$(dig @$SERVER -p $PORT $domain +timeout=2 2>/dev/null | grep "status:" | awk '{print $6}' | tr -d ',')
        if [[ "$status" == "NOERROR" ]]; then
            echo "  ✓ $domain → NOERROR"
            ((allowed_count++))
        else
            echo "  ✗ $domain → $status (expected NOERROR)"
        fi
    done
    
    echo ""
    echo "Blocked: $blocked_count/${#blocked_domains[@]}"
    echo "Allowed: $allowed_count/${#allowed_domains[@]}"
}

# --- Concurrent connection stress test ---
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

# --- Sustained load test ---
sustained_test() {
    echo ""
    echo "=== Sustained Load Test (60 seconds) ==="
    
    local duration=60
    local rate=1000  # target qps
    local total=0
    local errors=0
    
    local end_time=$(($(date +%s) + duration))
    
    while [[ $(date +%s) -lt $end_time ]]; do
        for i in $(seq 1 100); do
            result=$(dig @$SERVER -p $PORT google.com +short +timeout=1 2>/dev/null)
            if [[ -n "$result" ]]; then
                ((total++))
            else
                ((errors++))
            fi
        done &
    done
    
    wait
    
    local success_rate=$(echo "scale=2; ($total - $errors) * 100 / $total" | bc)
    
    echo "Total queries: $total"
    echo "Errors: $errors"
    echo "Success rate: ${success_rate}%"
    echo "Avg throughput: $(echo "$total / $duration" | bc) qps"
}

# --- Run all tests ---
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
    # sustained_test  # uncomment for long test
    
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
