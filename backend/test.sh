#!/bin/bash
# test_dns.sh

SERVER="127.0.0.1"
PORT="5354"
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo "=== DNS Proxy Test Suite ==="
echo "Server: $SERVER:$PORT"
echo ""

# Test counter
PASSED=0
FAILED=0

test_query() {
    local name="$1"
    local domain="$2"
    local expected_status="$3"
    local type="${4:-A}"
    
    result=$(dig @$SERVER -p $PORT $domain $type +short +timeout=2 2>/dev/null)
    status=$(dig @$SERVER -p $PORT $domain $type +timeout=2 2>/dev/null | grep "status:" | awk '{print $6}' | tr -d ',')
    
    if [[ "$status" == "$expected_status" ]]; then
        echo -e "${GREEN}✓${NC} $name"
        echo "  Query: $domain $type"
        echo "  Status: $status"
        [[ -n "$result" ]] && echo "  Result: $result"
        ((PASSED++))
    else
        echo -e "${RED}✗${NC} $name"
        echo "  Query: $domain $type"
        echo "  Expected: $expected_status"
        echo "  Got: $status"
        ((FAILED++))
    fi
    echo ""
}

test_latency() {
    local domain="$1"
    local queries="$2"
    
    echo "=== Latency Test: $queries queries to $domain ==="
    
    total=0
    for i in $(seq 1 $queries); do
        time=$(dig @$SERVER -p $PORT $domain +short +timeout=2 | head -1)
        latency=$(dig @$SERVER -p $PORT $domain +timeout=2 2>/dev/null | grep "Query time:" | awk '{print $4}')
        total=$((total + latency))
    done
    
    avg=$((total / queries))
    echo "Average latency: ${avg}ms"
    echo ""
}

# --- Basic Resolution Tests ---
echo "=== Basic Resolution ==="
test_query "A record resolution" "google.com" "NOERROR" "A"
test_query "AAAA record resolution" "google.com" "NOERROR" "AAAA"
test_query "MX record resolution" "google.com" "NOERROR" "MX"
test_query "CNAME resolution" "www.github.com" "NOERROR" "CNAME"

# --- Blocklist Tests ---
echo "=== Blocklist Tests ==="
test_query "Blocked domain returns NXDOMAIN" "ads.google.com" "NXDOMAIN" "A"
test_query "Blocked domain (doubleclick)" "doubleclick.net" "NXDOMAIN" "A"
test_query "Blocked subdomain" "tracking.facebook.com" "NXDOMAIN" "A"

# --- Cache Tests ---
echo "=== Cache Tests ==="
echo "First query (cache miss):"
time1=$(dig @$SERVER -p $PORT example.com A +timeout=2 2>/dev/null | grep "Query time:" | awk '{print $4}')
echo "  Latency: ${time1}ms"

echo "Second query (cache hit):"
time2=$(dig @$SERVER -p $PORT example.com A +timeout=2 2>/dev/null | grep "Query time:" | awk '{print $4}')
echo "  Latency: ${time2}ms"

if [[ $time2 -lt $time1 ]]; then
    echo -e "${GREEN}✓${NC} Cache working (${time2}ms < ${time1}ms)"
    ((PASSED++))
else
    echo -e "${RED}✗${NC} Cache may not be working"
    ((FAILED++))
fi
echo ""

# --- TCP Test ---
echo "=== TCP Test ==="
tcp_result=$(dig @$SERVER -p $PORT google.com A +tcp +short +timeout=2 2>/dev/null)
if [[ -n "$tcp_result" ]]; then
    echo -e "${GREEN}✓${NC} TCP query successful"
    echo "  Result: $tcp_result"
    ((PASSED++))
else
    echo -e "${RED}✗${NC} TCP query failed"
    ((FAILED++))
fi
echo ""

# --- Edge Cases ---
echo "=== Edge Cases ==="
test_query "Nonexistent domain" "thisdoesnotexist12345.com" "NXDOMAIN" "A"
test_query "Empty subdomain" "google.com" "NOERROR" "A"

# --- Latency Benchmark ---
test_latency "google.com" 10

# --- Summary ---
echo "=== Summary ==="
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"

if [[ $FAILED -eq 0 ]]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed${NC}"
    exit 1
fi
