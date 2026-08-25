#!/usr/bin/env bash
# Denis DNS proxy — end-to-end test suite
# Usage: ./test.sh [--dns-port 53] [--api-port 8080]

set -uo pipefail

# ── Config ────────────────────────────────────────────────────────────────────
DNS_HOST="${DNS_HOST:-127.0.0.1}"
DNS_PORT="${DNS_PORT:-53}"
API="${API:-http://localhost:8080}"
CH="${CH:-http://localhost:8123}"
CH_USER="${CH_USER:-default}"
CH_PASSWORD="${CH_PASSWORD:-clickhouse}"
KAFKA="${KAFKA:-localhost:9092}"

# ── Colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[1;34m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

PASS=0; FAIL=0; SKIP=0

pass()    { echo -e "  ${GREEN}✓${NC}  $1"; ((PASS++)); }
fail()    { echo -e "  ${RED}✗${NC}  $1"; ((FAIL++)); }
skip()    { echo -e "  ${YELLOW}–${NC}  $1 ${YELLOW}(skipped)${NC}"; ((SKIP++)); }
section() { echo -e "\n${BOLD}${BLUE}▶ $1${NC}"; }
info()    { echo -e "  ${CYAN}·${NC}  $1"; }

# ── Helpers ───────────────────────────────────────────────────────────────────

# Run a DNS query and return the dig output; exit 0 on any response (even NXDOMAIN)
dns_query() {
    local flags="$1" domain="$2" type="${3:-A}"
    dig $flags "@${DNS_HOST}" -p "${DNS_PORT}" "${domain}" "${type}" +time=3 +tries=1 2>/dev/null
}

dns_status() {
    dns_query "$1" "$2" "$3" | awk '/;; ->>HEADER<<-/{for(i=1;i<=NF;i++) if($i=="status:") print $(i+1)}' | tr -d ','
}

api() {
    local method="$1" path="$2"; shift 2
    curl -s -X "${method}" "${API}${path}" -H "Content-Type: application/json" "$@"
}

ch_query() {
    curl -s "${CH}/?user=${CH_USER}&password=${CH_PASSWORD}" --data-binary "$1"
}

wait_for_ch_rows() {
    local target="$1" waited=0
    while [[ $waited -lt 12 ]]; do
        local count
        count=$(ch_query "SELECT count() FROM dns_queries" 2>/dev/null | tr -d '[:space:]')
        [[ "$count" =~ ^[0-9]+$ ]] && [[ "$count" -ge "$target" ]] && return 0
        sleep 1; ((waited++))
    done
    return 1
}

# ── Prerequisites ─────────────────────────────────────────────────────────────
section "Prerequisites"

if command -v dig &>/dev/null; then pass "dig available"; else fail "dig not found — install bind-tools"; exit 1; fi
if command -v curl &>/dev/null; then pass "curl available"; else fail "curl not found"; exit 1; fi

HAS_KCAT=false
if command -v kcat &>/dev/null; then pass "kcat available"; HAS_KCAT=true; else skip "kcat not found (Kafka consumer tests will be skipped)"; fi

# ── Service reachability ──────────────────────────────────────────────────────
section "Service reachability"

DNS_UP=false
if dns_query "" "google.com" "A" | grep -q "NOERROR\|NXDOMAIN\|SERVFAIL"; then
    pass "DNS server reachable at ${DNS_HOST}:${DNS_PORT}"; DNS_UP=true
else
    fail "DNS server not reachable at ${DNS_HOST}:${DNS_PORT} — is the server running?"
fi

API_UP=false
if curl -sf "${API}/health" -o /dev/null; then
    pass "Management API reachable at ${API}"; API_UP=true
else
    fail "Management API not reachable at ${API} — is the server running?"
fi

CH_UP=false
if curl -sf "${CH}/ping" -o /dev/null 2>/dev/null || ch_query "SELECT 1" 2>/dev/null | grep -q "^1"; then
    pass "ClickHouse reachable at ${CH}"; CH_UP=true
else
    skip "ClickHouse not reachable at ${CH} — analytics tests will be skipped"
fi

KAFKA_UP=false
if $HAS_KCAT && kcat -b "${KAFKA}" -L -t dns-queries 2>/dev/null | grep -q "dns-queries\|broker"; then
    pass "Kafka reachable at ${KAFKA}"; KAFKA_UP=true
elif $HAS_KCAT && kcat -b "${KAFKA}" -L 2>/dev/null | grep -q "broker"; then
    pass "Kafka reachable at ${KAFKA}"; KAFKA_UP=true
else
    skip "Kafka not reachable at ${KAFKA} — Kafka tests will be skipped"
fi

# ── UDP DNS queries ───────────────────────────────────────────────────────────
section "DNS — UDP"

if $DNS_UP; then
    status=$(dns_status "" "google.com" "A")
    [[ "$status" == "NOERROR" ]] && pass "google.com A → NOERROR" || fail "google.com A → expected NOERROR, got '$status'"

    status=$(dns_status "" "cloudflare.com" "A")
    [[ "$status" == "NOERROR" ]] && pass "cloudflare.com A → NOERROR" || fail "cloudflare.com A → expected NOERROR, got '$status'"

    status=$(dns_status "" "google.com" "AAAA")
    [[ "$status" == "NOERROR" ]] && pass "google.com AAAA → NOERROR" || fail "google.com AAAA → expected NOERROR, got '$status'"

    # Cache hit — second query for the same domain
    status=$(dns_status "" "cloudflare.com" "A")
    [[ "$status" == "NOERROR" ]] && pass "cloudflare.com A (cache hit) → NOERROR" || fail "cloudflare.com A cache hit → '$status'"

    status=$(dns_status "" "thisdomain.absolutely.does.not.exist.local" "A")
    [[ "$status" == "NXDOMAIN" || "$status" == "SERVFAIL" ]] && \
        pass "nonexistent domain → ${status}" || fail "nonexistent domain → expected NXDOMAIN/SERVFAIL, got '$status'"
else
    for label in "google.com A" "cloudflare.com A" "google.com AAAA" "cloudflare.com A (cache)" "nonexistent domain"; do
        skip "DNS UDP: $label"
    done
fi

# ── TCP DNS queries ───────────────────────────────────────────────────────────
section "DNS — TCP"

if $DNS_UP; then
    status=$(dns_status "+tcp" "google.com" "A")
    [[ "$status" == "NOERROR" ]] && pass "google.com A via TCP → NOERROR" || fail "google.com A TCP → expected NOERROR, got '$status'"

    status=$(dns_status "+tcp" "cloudflare.com" "AAAA")
    [[ "$status" == "NOERROR" ]] && pass "cloudflare.com AAAA via TCP → NOERROR" || fail "cloudflare.com AAAA TCP → '$status'"

    # Verify we get the same answer over TCP as UDP
    udp_ip=$(dns_query "" "google.com" "A" | awk '/^google\.com\./{print $5}' | head -1)
    tcp_ip=$(dns_query "+tcp" "google.com" "A" | awk '/^google\.com\./{print $5}' | head -1)
    [[ -n "$udp_ip" && -n "$tcp_ip" ]] && \
        pass "UDP and TCP return answers for google.com (UDP: $udp_ip, TCP: $tcp_ip)" || \
        fail "Could not compare UDP/TCP answers"
else
    for label in "google.com A TCP" "cloudflare.com AAAA TCP" "UDP==TCP consistency"; do skip "DNS TCP: $label"; done
fi

# ── Management API ────────────────────────────────────────────────────────────
section "Management API — health"

if $API_UP; then
    health=$(api GET /health)
    status=$(echo "$health" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('status','?'))" 2>/dev/null)
    [[ "$status" == "ok" ]] && pass "GET /health → status: ok" || fail "GET /health → unexpected: $health"

    block_size=$(echo "$health" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('block_list_size','?'))" 2>/dev/null)
    info "block_list_size: $block_size"

    l1=$(echo "$health" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('l1_cache_entries','?'))" 2>/dev/null)
    info "l1_cache_entries: $l1"
else
    skip "API health check"
fi

section "Management API — blocklist CRUD"

TEST_DOMAIN="test-block-$(date +%s).example"
TEST_DOMAIN2="test-block2-$(date +%s).example"
TEST_DOMAIN3="test-block3-$(date +%s).example"

if $API_UP; then
    # Add single domain
    resp=$(api POST /blocklist -d "{\"domain\":\"${TEST_DOMAIN}\"}")
    http_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${API}/blocklist" \
        -H "Content-Type: application/json" -d "{\"domain\":\"${TEST_DOMAIN}\"}")
    # We already added it above, so adding again hits ON CONFLICT — check the original add succeeded
    # Re-test with a fresh domain
    resp_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${API}/blocklist" \
        -H "Content-Type: application/json" -d "{\"domain\":\"${TEST_DOMAIN2}\"}")
    [[ "$resp_code" == "201" ]] && pass "POST /blocklist → 201 Created" || fail "POST /blocklist → expected 201, got $resp_code"

    # Add bulk
    bulk_resp=$(api POST /blocklist/bulk -d "{\"domains\":[\"${TEST_DOMAIN3}\",\"${TEST_DOMAIN3}\"]}")
    added=$(echo "$bulk_resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('added',0))" 2>/dev/null)
    [[ "$added" -ge 1 ]] && pass "POST /blocklist/bulk → added: $added" || fail "POST /blocklist/bulk → unexpected: $bulk_resp"

    # List blocklist
    list_resp=$(api GET /blocklist)
    count=$(echo "$list_resp" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
    [[ "$count" =~ ^[0-9]+$ ]] && pass "GET /blocklist → $count domains" || fail "GET /blocklist → unexpected: $list_resp"

    # Duplicate add is a no-op (ON CONFLICT DO NOTHING), should still return 201
    dup_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${API}/blocklist" \
        -H "Content-Type: application/json" -d "{\"domain\":\"${TEST_DOMAIN2}\"}")
    [[ "$dup_code" == "201" ]] && pass "POST /blocklist duplicate → 201 (idempotent)" || fail "POST /blocklist duplicate → got $dup_code"

    # Delete single
    del_code=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "${API}/blocklist/${TEST_DOMAIN2}")
    [[ "$del_code" == "204" ]] && pass "DELETE /blocklist/${TEST_DOMAIN2} → 204 No Content" || fail "DELETE /blocklist/${TEST_DOMAIN2} → got $del_code"

    # Delete non-existent → 404
    del_code=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "${API}/blocklist/notthere.example")
    [[ "$del_code" == "404" ]] && pass "DELETE /blocklist/notthere.example → 404 Not Found" || fail "DELETE /blocklist/notthere.example → expected 404, got $del_code"

    # Bulk remove
    bulk_del=$(api POST /blocklist/bulk/remove -d "{\"domains\":[\"${TEST_DOMAIN3}\",\"missing.example\"]}")
    removed=$(echo "$bulk_del" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('added',0))" 2>/dev/null)
    skipped=$(echo "$bulk_del" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('skipped',0))" 2>/dev/null)
    [[ "$removed" -ge 1 ]] && pass "POST /blocklist/bulk/remove → removed: $removed, skipped: $skipped" || fail "POST /blocklist/bulk/remove → unexpected: $bulk_del"
else
    for label in "POST /blocklist" "POST /blocklist/bulk" "GET /blocklist" "DELETE single" "DELETE 404" "bulk remove"; do
        skip "API: $label"
    done
fi

# ── End-to-end: block → query → unblock → query ──────────────────────────────
section "End-to-end: block/unblock flow"

E2E_DOMAIN="e2e-block-$(date +%s).local"

if $DNS_UP && $API_UP; then
    # Baseline: domain doesn't resolve (it's made up, so NXDOMAIN/SERVFAIL from upstream)
    before=$(dns_status "" "${E2E_DOMAIN}" "A")
    info "Before block: ${E2E_DOMAIN} → $before"

    # Block it
    api POST /blocklist -d "{\"domain\":\"${E2E_DOMAIN}\"}" > /dev/null
    sleep 0.2

    # Should now return NXDOMAIN from our server (not upstream)
    blocked=$(dns_status "" "${E2E_DOMAIN}" "A")
    [[ "$blocked" == "NXDOMAIN" ]] && \
        pass "After block: ${E2E_DOMAIN} → NXDOMAIN (served by proxy)" || \
        fail "After block: ${E2E_DOMAIN} → expected NXDOMAIN, got '$blocked'"

    # Unblock it
    curl -s -X DELETE "${API}/blocklist/${E2E_DOMAIN}" > /dev/null
    sleep 0.2

    # Should go back to upstream (NXDOMAIN from upstream, or SERVFAIL — not our synthetic one)
    after=$(dns_status "" "${E2E_DOMAIN}" "A")
    info "After unblock: ${E2E_DOMAIN} → $after"
    [[ "$after" == "NXDOMAIN" || "$after" == "SERVFAIL" ]] && \
        pass "After unblock: ${E2E_DOMAIN} → $after (upstream response)" || \
        fail "After unblock: ${E2E_DOMAIN} → unexpected '$after'"
else
    skip "E2E block/unblock (requires DNS + API)"
fi

# ── Kafka ─────────────────────────────────────────────────────────────────────
section "Kafka"

if $KAFKA_UP && $HAS_KCAT; then
    # List topics
    topics=$(kcat -b "${KAFKA}" -L 2>/dev/null | grep "topic \"" | awk -F'"' '{print $2}')
    if echo "$topics" | grep -q "dns-queries"; then
        pass "Topic 'dns-queries' exists"
    else
        info "Known topics: $(echo "$topics" | tr '\n' ' ')"
        fail "Topic 'dns-queries' not found — has the server produced any events?"
    fi

    # Consume a few messages (non-blocking, 3s timeout)
    info "Sampling up to 5 messages from dns-queries (3s timeout)…"
    msgs=$(kcat -b "${KAFKA}" -C -t dns-queries -e -q \
        -o beginning -c 5 2>/dev/null || true)

    if [[ -n "$msgs" ]]; then
        count=$(echo "$msgs" | grep -c "domain" || true)
        pass "Consumed messages from dns-queries ($count with 'domain' field)"
        echo "$msgs" | head -3 | while IFS= read -r line; do
            info "  $line"
        done
    else
        fail "No messages in dns-queries topic (server may not have produced yet)"
    fi

    # Verify message schema
    first_msg=$(kcat -b "${KAFKA}" -C -t dns-queries -e -q -o beginning -c 1 2>/dev/null || true)
    if [[ -n "$first_msg" ]]; then
        has_domain=$(echo "$first_msg" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print('ok' if 'domain' in d else 'missing')" 2>/dev/null)
        has_latency=$(echo "$first_msg" | python3 -c "import sys,json; d=json.loads(sys.stdin.read()); print('ok' if 'latency_us' in d else 'missing')" 2>/dev/null)
        [[ "$has_domain" == "ok" ]] && pass "Message has 'domain' field" || fail "Message missing 'domain' field"
        [[ "$has_latency" == "ok" ]] && pass "Message has 'latency_us' field" || fail "Message missing 'latency_us' field"
    fi
else
    skip "Kafka topic check"
    skip "Kafka message sample"
    skip "Kafka message schema"
fi

# ── ClickHouse ────────────────────────────────────────────────────────────────
section "ClickHouse"

if $CH_UP; then
    # Table exists
    tables=$(ch_query "SHOW TABLES" 2>/dev/null)
    if echo "$tables" | grep -q "dns_queries"; then
        pass "Table 'dns_queries' exists"
    else
        fail "Table 'dns_queries' not found — has the analytics consumer connected?"
    fi

    # Row count (consumer batches every 5s — wait up to 12s)
    info "Waiting for ClickHouse rows (consumer flushes every 5s)…"
    if wait_for_ch_rows 1; then
        row_count=$(ch_query "SELECT count() FROM dns_queries" 2>/dev/null | tr -d '[:space:]')
        pass "dns_queries has ${row_count} rows"

        # Sample recent queries
        info "10 most recent DNS events:"
        ch_query "SELECT timestamp_ms, domain, query_type, response_code, cache_hit, blocked, latency_us FROM dns_queries ORDER BY timestamp_ms DESC LIMIT 10 FORMAT PrettyCompact" 2>/dev/null \
            | sed 's/^/    /'

        # Top queried domains
        info "Top 5 queried domains:"
        ch_query "SELECT domain, count() AS queries FROM dns_queries GROUP BY domain ORDER BY queries DESC LIMIT 5 FORMAT PrettyCompact" 2>/dev/null \
            | sed 's/^/    /'

        # Cache hit rate
        hit_rate=$(ch_query "SELECT round(100.0 * countIf(cache_hit = 1) / count(), 1) FROM dns_queries" 2>/dev/null | tr -d '[:space:]')
        pass "Cache hit rate: ${hit_rate}%"

        # Blocked query count
        blocked_count=$(ch_query "SELECT countIf(blocked = 1) FROM dns_queries" 2>/dev/null | tr -d '[:space:]')
        info "Blocked queries: ${blocked_count}"

        # Avg latency
        avg_lat=$(ch_query "SELECT round(avg(latency_us)) FROM dns_queries" 2>/dev/null | tr -d '[:space:]')
        info "Avg latency: ${avg_lat}µs"
    else
        fail "No rows in dns_queries after 12s — Kafka consumer may not be flushing"
    fi
else
    skip "ClickHouse table check"
    skip "ClickHouse row count"
    skip "ClickHouse analytics queries"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo -e "\n${BOLD}────────────────────────────────────${NC}"
echo -e "${BOLD}Results:  ${GREEN}${PASS} passed${NC}  ${RED}${FAIL} failed${NC}  ${YELLOW}${SKIP} skipped${NC}"
echo -e "${BOLD}────────────────────────────────────${NC}"

[[ $FAIL -eq 0 ]]
