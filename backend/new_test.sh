#!/usr/bin/env bash
#
# DNS Proxy Comprehensive Benchmark Suite
# Tests: QPS throughput, latency percentiles, concurrency, cache efficiency, stability
#
# Dependencies: dnsperf, dnstop (optional), dig, bc, awk
# Install: sudo apt install dnsperf bind9-dnsutils bc
#

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SERVER="${DNS_SERVER:-127.0.0.1}"
PORT="${DNS_PORT:-5354}"
RESULTS_DIR="${RESULTS_DIR:-./dns_benchmark_results}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Test parameters
THROUGHPUT_DURATION=30          # seconds
THROUGHPUT_CLIENTS=50           # concurrent clients for dnsperf
THROUGHPUT_TARGET_QPS=100000    # target QPS limit

LATENCY_SAMPLES=1000            # number of queries for latency measurement
CONCURRENCY_LEVELS=(1 10 50 100 200 500 1000)
SUSTAINED_DURATION=300          # 5 minutes sustained load
RAMP_STEPS=(100 500 1000 2000 5000 10000 20000 50000)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ============================================================================
# Helper Functions
# ============================================================================

log_info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[PASS]${NC} $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error()   { echo -e "${RED}[FAIL]${NC} $*"; }
log_header()  { echo -e "\n${CYAN}══════════════════════════════════════════════════════════════${NC}"; echo -e "${CYAN}  $*${NC}"; echo -e "${CYAN}══════════════════════════════════════════════════════════════${NC}"; }

check_dependencies() {
    local missing=()
    
    for cmd in dnsperf dig bc awk sort; do
        if ! command -v "$cmd" &> /dev/null; then
            missing+=("$cmd")
        fi
    done
    
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing dependencies: ${missing[*]}"
        echo "Install with: sudo apt install dnsperf bind9-dnsutils bc gawk"
        exit 1
    fi
    
    log_success "All dependencies found"
}

check_server() {
    log_info "Checking DNS server at $SERVER:$PORT..."
    
    local result
    result=$(dig @"$SERVER" -p "$PORT" example.com +short +timeout=5 2>/dev/null) || true
    
    if [[ -z "$result" ]]; then
        log_error "DNS server not responding at $SERVER:$PORT"
        exit 1
    fi
    
    log_success "Server responding (example.com → $result)"
}

setup_results_dir() {
    mkdir -p "$RESULTS_DIR"
    RESULT_FILE="$RESULTS_DIR/benchmark_$TIMESTAMP.txt"
    CSV_FILE="$RESULTS_DIR/benchmark_$TIMESTAMP.csv"
    
    log_info "Results will be saved to $RESULTS_DIR"
}

generate_query_file() {
    local file="$1"
    local count="${2:-10000}"
    
    # Common domains for realistic testing
    local domains=(
        "google.com" "facebook.com" "amazon.com" "youtube.com" "twitter.com"
        "reddit.com" "github.com" "stackoverflow.com" "linkedin.com" "netflix.com"
        "microsoft.com" "apple.com" "cloudflare.com" "wikipedia.org" "yahoo.com"
        "instagram.com" "whatsapp.com" "zoom.us" "slack.com" "dropbox.com"
        "example.com" "example.org" "example.net" "test.com" "localhost.com"
    )
    
    > "$file"
    for ((i=0; i<count; i++)); do
        echo "${domains[$((i % ${#domains[@]}))]}" A >> "$file"
    done
    
    log_info "Generated query file with $count entries"
}

# ============================================================================
# Test: Basic Connectivity & Response Validation
# ============================================================================

test_connectivity() {
    log_header "Test 1: Connectivity & Response Validation"
    
    local test_domains=("google.com" "cloudflare.com" "github.com" "example.com")
    local passed=0
    local total=${#test_domains[@]}
    
    for domain in "${test_domains[@]}"; do
        local result status rcode
        result=$(dig @"$SERVER" -p "$PORT" "$domain" +timeout=5 2>/dev/null)
        rcode=$(echo "$result" | grep -oP 'status: \K[A-Z]+' || echo "TIMEOUT")
        
        if [[ "$rcode" == "NOERROR" ]]; then
            log_success "$domain → NOERROR"
            ((passed++))
        else
            log_error "$domain → $rcode"
        fi
    done
    
    echo ""
    echo "Connectivity: $passed/$total domains resolved"
    echo "connectivity,$passed,$total" >> "$CSV_FILE"
}

# ============================================================================
# Test: Throughput (QPS) with dnsperf
# ============================================================================

test_throughput() {
    log_header "Test 2: Throughput (QPS)"
    
    local query_file="/tmp/dns_queries_$$.txt"
    generate_query_file "$query_file" 10000
    
    log_info "Running dnsperf for ${THROUGHPUT_DURATION}s with $THROUGHPUT_CLIENTS clients..."
    
    local output
    output=$(dnsperf -s "$SERVER" -p "$PORT" \
        -d "$query_file" \
        -l "$THROUGHPUT_DURATION" \
        -c "$THROUGHPUT_CLIENTS" \
        -Q "$THROUGHPUT_TARGET_QPS" \
        -S 1 2>&1) || true
    
    # Parse results
    local qps completed lost avg_latency
    qps=$(echo "$output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
    completed=$(echo "$output" | grep -oP 'Queries completed:\s+\K\d+' || echo "0")
    lost=$(echo "$output" | grep -oP 'Queries lost:\s+\K\d+' || echo "0")
    avg_latency=$(echo "$output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
    
    # Convert latency to ms
    local avg_latency_ms
    avg_latency_ms=$(echo "scale=3; $avg_latency * 1000" | bc)
    
    echo ""
    echo "Results:"
    echo "  Queries per second: $qps"
    echo "  Completed: $completed"
    echo "  Lost: $lost"
    echo "  Average latency: ${avg_latency_ms}ms"
    
    # Assessment
    if (( $(echo "$qps > 10000" | bc -l) )); then
        log_success "Throughput exceeds 10k QPS"
    elif (( $(echo "$qps > 5000" | bc -l) )); then
        log_warn "Throughput between 5k-10k QPS"
    else
        log_error "Throughput below 5k QPS"
    fi
    
    echo "throughput,$qps,$completed,$lost,$avg_latency_ms" >> "$CSV_FILE"
    
    rm -f "$query_file"
}

# ============================================================================
# Test: Latency Percentiles
# ============================================================================

test_latency() {
    log_header "Test 3: Latency Percentiles"
    
    local latency_file="/tmp/dns_latencies_$$.txt"
    > "$latency_file"
    
    log_info "Collecting $LATENCY_SAMPLES latency samples..."
    
    local domains=("google.com" "github.com" "cloudflare.com" "example.com")
    
    for ((i=0; i<LATENCY_SAMPLES; i++)); do
        local domain="${domains[$((i % ${#domains[@]}))]}"
        
        # Use dig's query time
        local result
        result=$(dig @"$SERVER" -p "$PORT" "$domain" +timeout=5 2>/dev/null | grep "Query time:" | awk '{print $4}')
        
        if [[ -n "$result" ]]; then
            echo "$result" >> "$latency_file"
        fi
        
        # Progress indicator every 100 samples
        if (( i % 100 == 0 && i > 0 )); then
            echo -ne "\r  Progress: $i/$LATENCY_SAMPLES"
        fi
    done
    echo -ne "\r  Progress: $LATENCY_SAMPLES/$LATENCY_SAMPLES\n"
    
    # Calculate percentiles
    local count p50 p90 p95 p99 p999 min max avg
    count=$(wc -l < "$latency_file")
    
    if [[ "$count" -lt 10 ]]; then
        log_error "Insufficient samples collected ($count)"
        return
    fi
    
    # Sort and calculate percentiles
    sort -n "$latency_file" -o "$latency_file"
    
    min=$(head -1 "$latency_file")
    max=$(tail -1 "$latency_file")
    avg=$(awk '{sum+=$1} END {printf "%.2f", sum/NR}' "$latency_file")
    
    p50=$(sed -n "$((count * 50 / 100))p" "$latency_file")
    p90=$(sed -n "$((count * 90 / 100))p" "$latency_file")
    p95=$(sed -n "$((count * 95 / 100))p" "$latency_file")
    p99=$(sed -n "$((count * 99 / 100))p" "$latency_file")
    p999=$(sed -n "$((count * 999 / 1000))p" "$latency_file")
    
    echo ""
    echo "Latency Distribution (ms):"
    echo "  Min:    ${min}ms"
    echo "  Avg:    ${avg}ms"
    echo "  P50:    ${p50}ms"
    echo "  P90:    ${p90}ms"
    echo "  P95:    ${p95}ms"
    echo "  P99:    ${p99}ms"
    echo "  P99.9:  ${p999}ms"
    echo "  Max:    ${max}ms"
    
    # Assessment
    if (( p99 <= 5 )); then
        log_success "P99 latency ≤ 5ms (excellent)"
    elif (( p99 <= 10 )); then
        log_warn "P99 latency ≤ 10ms (good)"
    else
        log_error "P99 latency > 10ms (needs improvement)"
    fi
    
    echo "latency,$min,$avg,$p50,$p90,$p95,$p99,$p999,$max" >> "$CSV_FILE"
    
    rm -f "$latency_file"
}

# ============================================================================
# Test: Latency Under Load (using dnsperf histogram)
# ============================================================================

test_latency_under_load() {
    log_header "Test 4: Latency Under Load"
    
    local query_file="/tmp/dns_queries_load_$$.txt"
    generate_query_file "$query_file" 5000
    
    # Test at different load levels
    local load_levels=(10 50 100)
    
    for clients in "${load_levels[@]}"; do
        log_info "Testing with $clients concurrent clients..."
        
        local output
        output=$(dnsperf -s "$SERVER" -p "$PORT" \
            -d "$query_file" \
            -l 10 \
            -c "$clients" \
            -S 1 2>&1) || true
        
        local qps avg_latency min_latency max_latency
        qps=$(echo "$output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
        avg_latency=$(echo "$output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
        min_latency=$(echo "$output" | grep -oP 'min \K[\d.]+' || echo "0")
        max_latency=$(echo "$output" | grep -oP 'max \K[\d.]+' || echo "0")
        
        local avg_ms min_ms max_ms
        avg_ms=$(echo "scale=3; $avg_latency * 1000" | bc)
        min_ms=$(echo "scale=3; $min_latency * 1000" | bc)
        max_ms=$(echo "scale=3; $max_latency * 1000" | bc)
        
        echo "  $clients clients: ${qps} qps, latency: ${avg_ms}ms avg (${min_ms}-${max_ms}ms)"
        echo "latency_under_load,$clients,$qps,$avg_ms,$min_ms,$max_ms" >> "$CSV_FILE"
    done
    
    rm -f "$query_file"
}

# ============================================================================
# Test: Concurrency Scaling
# ============================================================================

test_concurrency() {
    log_header "Test 5: Concurrency Scaling"
    
    local query_file="/tmp/dns_queries_conc_$$.txt"
    generate_query_file "$query_file" 5000
    
    echo "Testing QPS at different concurrency levels..."
    echo ""
    printf "%-12s %-12s %-15s %-12s\n" "Clients" "QPS" "Avg Latency" "Lost"
    printf "%-12s %-12s %-15s %-12s\n" "-------" "---" "-----------" "----"
    
    for clients in "${CONCURRENCY_LEVELS[@]}"; do
        local output
        output=$(dnsperf -s "$SERVER" -p "$PORT" \
            -d "$query_file" \
            -l 10 \
            -c "$clients" \
            -Q 100000 \
            2>&1) || true
        
        local qps lost avg_latency
        qps=$(echo "$output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
        lost=$(echo "$output" | grep -oP 'Queries lost:\s+\K\d+' || echo "0")
        avg_latency=$(echo "$output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
        
        local avg_ms
        avg_ms=$(echo "scale=3; $avg_latency * 1000" | bc)
        
        printf "%-12s %-12s %-15s %-12s\n" "$clients" "${qps%.*}" "${avg_ms}ms" "$lost"
        echo "concurrency,$clients,$qps,$avg_ms,$lost" >> "$CSV_FILE"
    done
    
    rm -f "$query_file"
}

# ============================================================================
# Test: Ramp-up / Breaking Point
# ============================================================================

test_ramp() {
    log_header "Test 6: Ramp-up / Breaking Point Detection"
    
    local query_file="/tmp/dns_queries_ramp_$$.txt"
    generate_query_file "$query_file" 10000
    
    echo "Finding maximum sustainable QPS..."
    echo ""
    printf "%-15s %-12s %-12s %-15s\n" "Target QPS" "Actual QPS" "Lost %" "Latency"
    printf "%-15s %-12s %-12s %-15s\n" "----------" "----------" "------" "-------"
    
    local max_sustainable_qps=0
    local breaking_point=0
    
    for target in "${RAMP_STEPS[@]}"; do
        local output
        output=$(dnsperf -s "$SERVER" -p "$PORT" \
            -d "$query_file" \
            -l 10 \
            -c 100 \
            -Q "$target" \
            2>&1) || true
        
        local qps sent lost lost_pct avg_latency
        qps=$(echo "$output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
        sent=$(echo "$output" | grep -oP 'Queries sent:\s+\K\d+' || echo "0")
        lost=$(echo "$output" | grep -oP 'Queries lost:\s+\K\d+' || echo "0")
        avg_latency=$(echo "$output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
        
        if [[ "$sent" -gt 0 ]]; then
            lost_pct=$(echo "scale=2; $lost * 100 / $sent" | bc)
        else
            lost_pct="100"
        fi
        
        local avg_ms
        avg_ms=$(echo "scale=3; $avg_latency * 1000" | bc)
        
        printf "%-15s %-12s %-12s %-15s\n" "$target" "${qps%.*}" "${lost_pct}%" "${avg_ms}ms"
        echo "ramp,$target,$qps,$lost_pct,$avg_ms" >> "$CSV_FILE"
        
        # Track maximum sustainable (< 1% loss)
        if (( $(echo "$lost_pct < 1" | bc -l) )); then
            max_sustainable_qps=${qps%.*}
        elif [[ "$breaking_point" -eq 0 ]]; then
            breaking_point=$target
        fi
    done
    
    echo ""
    log_info "Maximum sustainable QPS (< 1% loss): $max_sustainable_qps"
    if [[ "$breaking_point" -gt 0 ]]; then
        log_info "Breaking point reached at target: $breaking_point QPS"
    fi
    
    rm -f "$query_file"
}

# ============================================================================
# Test: Cache Efficiency
# ============================================================================

test_cache() {
    log_header "Test 7: Cache Efficiency"
    
    local query_file="/tmp/dns_queries_cache_$$.txt"
    
    # Small set of domains to ensure high cache hit rate potential
    local domains=("google.com" "github.com" "cloudflare.com")
    
    > "$query_file"
    for ((i=0; i<1000; i++)); do
        echo "${domains[$((i % ${#domains[@]}))]}" A >> "$query_file"
    done
    
    log_info "Cold cache test (first run)..."
    local cold_output
    cold_output=$(dnsperf -s "$SERVER" -p "$PORT" \
        -d "$query_file" \
        -l 5 \
        -c 10 \
        2>&1) || true
    
    local cold_qps cold_latency
    cold_qps=$(echo "$cold_output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
    cold_latency=$(echo "$cold_output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
    cold_latency_ms=$(echo "scale=3; $cold_latency * 1000" | bc)
    
    log_info "Warm cache test (second run)..."
    local warm_output
    warm_output=$(dnsperf -s "$SERVER" -p "$PORT" \
        -d "$query_file" \
        -l 5 \
        -c 10 \
        2>&1) || true
    
    local warm_qps warm_latency
    warm_qps=$(echo "$warm_output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
    warm_latency=$(echo "$warm_output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
    warm_latency_ms=$(echo "scale=3; $warm_latency * 1000" | bc)
    
    echo ""
    echo "Cache Performance Comparison:"
    echo "  Cold cache: ${cold_qps%.*} QPS, ${cold_latency_ms}ms latency"
    echo "  Warm cache: ${warm_qps%.*} QPS, ${warm_latency_ms}ms latency"
    
    local speedup
    if (( $(echo "$cold_qps > 0" | bc -l) )); then
        speedup=$(echo "scale=2; $warm_qps / $cold_qps" | bc)
        echo "  Speedup: ${speedup}x"
    fi
    
    local latency_improvement
    if (( $(echo "$cold_latency > 0" | bc -l) )); then
        latency_improvement=$(echo "scale=2; ($cold_latency - $warm_latency) / $cold_latency * 100" | bc)
        echo "  Latency improvement: ${latency_improvement}%"
    fi
    
    echo "cache,$cold_qps,$cold_latency_ms,$warm_qps,$warm_latency_ms" >> "$CSV_FILE"
    
    rm -f "$query_file"
}

# ============================================================================
# Test: Query Type Support
# ============================================================================

test_query_types() {
    log_header "Test 8: Query Type Support"
    
    local record_types=("A" "AAAA" "CNAME" "MX" "TXT" "NS" "SOA" "PTR")
    local domain="google.com"
    
    echo "Testing different record types against $domain..."
    echo ""
    
    for qtype in "${record_types[@]}"; do
        local result status latency
        result=$(dig @"$SERVER" -p "$PORT" "$domain" "$qtype" +timeout=5 2>/dev/null)
        status=$(echo "$result" | grep -oP 'status: \K[A-Z]+' || echo "TIMEOUT")
        latency=$(echo "$result" | grep "Query time:" | awk '{print $4}')
        
        if [[ "$status" == "NOERROR" || "$status" == "NXDOMAIN" ]]; then
            log_success "$qtype: $status (${latency}ms)"
        else
            log_warn "$qtype: $status"
        fi
        
        echo "query_type,$qtype,$status,$latency" >> "$CSV_FILE"
    done
}

# ============================================================================
# Test: EDNS Support
# ============================================================================

test_edns() {
    log_header "Test 9: EDNS Support"
    
    log_info "Testing EDNS0 support..."
    local result
    result=$(dig @"$SERVER" -p "$PORT" google.com +edns=0 +timeout=5 2>/dev/null)
    
    if echo "$result" | grep -q "EDNS:"; then
        log_success "EDNS0 supported"
        local edns_version
        edns_version=$(echo "$result" | grep "EDNS:" | head -1)
        echo "  $edns_version"
    else
        log_warn "EDNS0 not detected in response"
    fi
    
    log_info "Testing DNSSEC (DO bit)..."
    result=$(dig @"$SERVER" -p "$PORT" google.com +dnssec +timeout=5 2>/dev/null)
    
    if echo "$result" | grep -q "RRSIG"; then
        log_success "DNSSEC signatures returned"
    else
        log_warn "No DNSSEC signatures in response"
    fi
}

# ============================================================================
# Test: Error Handling
# ============================================================================

test_error_handling() {
    log_header "Test 10: Error Handling"
    
    echo "Testing malformed/edge-case queries..."
    echo ""
    
    # Non-existent domain
    local result status
    result=$(dig @"$SERVER" -p "$PORT" thisdoesnotexist12345.invalid +timeout=5 2>/dev/null)
    status=$(echo "$result" | grep -oP 'status: \K[A-Z]+' || echo "TIMEOUT")
    if [[ "$status" == "NXDOMAIN" ]]; then
        log_success "NXDOMAIN for non-existent domain"
    else
        log_warn "Expected NXDOMAIN, got $status"
    fi
    
    # Empty query (should fail gracefully)
    result=$(dig @"$SERVER" -p "$PORT" "" +timeout=5 2>/dev/null)
    status=$(echo "$result" | grep -oP 'status: \K[A-Z]+' || echo "TIMEOUT")
    log_info "Empty query response: $status"
    
    # Very long domain name
    local long_domain
    long_domain=$(printf 'a%.0s' {1..63}).com
    result=$(dig @"$SERVER" -p "$PORT" "$long_domain" +timeout=5 2>/dev/null)
    status=$(echo "$result" | grep -oP 'status: \K[A-Z]+' || echo "TIMEOUT")
    log_info "Long domain (63 chars) response: $status"
}

# ============================================================================
# Test: TCP Fallback
# ============================================================================

test_tcp() {
    log_header "Test 11: TCP Support"
    
    log_info "Testing TCP connection..."
    local result status latency
    result=$(dig @"$SERVER" -p "$PORT" google.com +tcp +timeout=5 2>/dev/null)
    status=$(echo "$result" | grep -oP 'status: \K[A-Z]+' || echo "TIMEOUT")
    latency=$(echo "$result" | grep "Query time:" | awk '{print $4}')
    
    if [[ "$status" == "NOERROR" ]]; then
        log_success "TCP query successful (${latency}ms)"
    else
        log_warn "TCP query failed: $status"
    fi
    
    echo "tcp,$status,$latency" >> "$CSV_FILE"
}

# ============================================================================
# Test: Sustained Load Stability
# ============================================================================

test_sustained() {
    log_header "Test 12: Sustained Load (${SUSTAINED_DURATION}s)"
    
    local query_file="/tmp/dns_queries_sustained_$$.txt"
    generate_query_file "$query_file" 10000
    
    log_info "Running sustained load test for ${SUSTAINED_DURATION} seconds..."
    log_info "Sampling every 10 seconds..."
    echo ""
    
    printf "%-10s %-12s %-12s %-15s\n" "Time" "QPS" "Lost" "Latency"
    printf "%-10s %-12s %-12s %-15s\n" "----" "---" "----" "-------"
    
    local interval=10
    local iterations=$((SUSTAINED_DURATION / interval))
    local total_qps=0
    local total_lost=0
    local samples=0
    
    for ((i=1; i<=iterations; i++)); do
        local output
        output=$(dnsperf -s "$SERVER" -p "$PORT" \
            -d "$query_file" \
            -l "$interval" \
            -c 50 \
            -Q 50000 \
            2>&1) || true
        
        local qps lost avg_latency
        qps=$(echo "$output" | grep -oP 'Queries per second:\s+\K[\d.]+' || echo "0")
        lost=$(echo "$output" | grep -oP 'Queries lost:\s+\K\d+' || echo "0")
        avg_latency=$(echo "$output" | grep -oP 'Average Latency \(s\):\s+\K[\d.]+' || echo "0")
        
        local avg_ms
        avg_ms=$(echo "scale=3; $avg_latency * 1000" | bc)
        local elapsed=$((i * interval))
        
        printf "%-10s %-12s %-12s %-15s\n" "${elapsed}s" "${qps%.*}" "$lost" "${avg_ms}ms"
        echo "sustained,$elapsed,$qps,$lost,$avg_ms" >> "$CSV_FILE"
        
        total_qps=$(echo "$total_qps + $qps" | bc)
        total_lost=$((total_lost + lost))
        ((samples++))
    done
    
    local avg_qps
    avg_qps=$(echo "scale=2; $total_qps / $samples" | bc)
    
    echo ""
    echo "Sustained Test Summary:"
    echo "  Average QPS: ${avg_qps%.*}"
    echo "  Total queries lost: $total_lost"
    
    if [[ "$total_lost" -eq 0 ]]; then
        log_success "Zero packet loss during sustained load"
    else
        log_warn "$total_lost packets lost during sustained load"
    fi
    
    rm -f "$query_file"
}

# ============================================================================
# Generate Summary Report
# ============================================================================

generate_report() {
    log_header "Benchmark Summary Report"
    
    echo ""
    echo "Timestamp: $TIMESTAMP"
    echo "Server: $SERVER:$PORT"
    echo ""
    echo "Results saved to:"
    echo "  - $RESULT_FILE"
    echo "  - $CSV_FILE"
    echo ""
    
    # Parse CSV for key metrics
    if [[ -f "$CSV_FILE" ]]; then
        local throughput_qps latency_p99
        throughput_qps=$(grep "^throughput," "$CSV_FILE" | cut -d',' -f2 | head -1)
        latency_p99=$(grep "^latency," "$CSV_FILE" | cut -d',' -f7 | head -1)
        
        echo "Key Metrics:"
        echo "  Peak Throughput: ${throughput_qps:-N/A} QPS"
        echo "  P99 Latency: ${latency_p99:-N/A}ms"
    fi
}

# ============================================================================
# Main
# ============================================================================

usage() {
    cat << EOF
DNS Proxy Benchmark Suite

Usage: $0 [OPTIONS] [TESTS...]

Options:
    -s, --server HOST    DNS server address (default: 127.0.0.1)
    -p, --port PORT      DNS server port (default: 5354)
    -o, --output DIR     Output directory (default: ./dns_benchmark_results)
    -h, --help           Show this help

Tests (run all if none specified):
    connectivity     Basic connectivity check
    throughput       QPS measurement with dnsperf
    latency          Latency percentile distribution
    latency-load     Latency under various load levels
    concurrency      Concurrency scaling test
    ramp             Ramp-up / breaking point detection
    cache            Cache efficiency comparison
    query-types      Query type support (A, AAAA, MX, etc.)
    edns             EDNS and DNSSEC support
    errors           Error handling test
    tcp              TCP fallback support
    sustained        Long-running stability test

Examples:
    $0                          # Run all tests
    $0 throughput latency       # Run specific tests
    $0 -s 192.168.1.1 -p 53     # Custom server
    DNS_SERVER=10.0.0.1 $0      # Environment variable

EOF
}

main() {
    local tests=()
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -s|--server) SERVER="$2"; shift 2 ;;
            -p|--port) PORT="$2"; shift 2 ;;
            -o|--output) RESULTS_DIR="$2"; shift 2 ;;
            -h|--help) usage; exit 0 ;;
            -*) echo "Unknown option: $1"; usage; exit 1 ;;
            *) tests+=("$1"); shift ;;
        esac
    done
    
    # Banner
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║           DNS Proxy Comprehensive Benchmark Suite            ║"
    echo "║                                                              ║"
    echo "║  Server: $SERVER:$PORT"
    echo "╚══════════════════════════════════════════════════════════════╝"
    
    # Setup
    check_dependencies
    setup_results_dir
    check_server
    
    # Initialize CSV
    echo "test,metric1,metric2,metric3,metric4,metric5" > "$CSV_FILE"
    
    # Run tests
    if [[ ${#tests[@]} -eq 0 ]]; then
        # Run all tests
        test_connectivity
        test_throughput
        test_latency
        test_latency_under_load
        test_concurrency
        test_ramp
        test_cache
        test_query_types
        test_edns
        test_error_handling
        test_tcp
        test_sustained
    else
        # Run selected tests
        for test in "${tests[@]}"; do
            case "$test" in
                connectivity) test_connectivity ;;
                throughput) test_throughput ;;
                latency) test_latency ;;
                latency-load) test_latency_under_load ;;
                concurrency) test_concurrency ;;
                ramp) test_ramp ;;
                cache) test_cache ;;
                query-types) test_query_types ;;
                edns) test_edns ;;
                errors) test_error_handling ;;
                tcp) test_tcp ;;
                sustained) test_sustained ;;
                *) echo "Unknown test: $test"; usage; exit 1 ;;
            esac
        done
    fi
    
    generate_report
}

main "$@"
