# 1
╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Fri Jan 23 18:33:44 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         79852
  Queries completed:    79852 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 79852 (100.00%)
  Average packet size:  request 28, response 56
  Run time (s):         10.008649
  Queries per second:   7978.299569

  Average Latency (s):  0.012452 (min 0.003685, max 0.096187)
  Latency StdDev (s):   0.001276


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 2.02ms
  P50: 2ms
  P95: 3ms
  P99: 5ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 980 cache hits
Cache hit rate: 98.00%
✓ 85% cache hit rate achieved

=== Blocklist Test ===
Testing blocked domains:
  ✗ ads.google.com → NOERROR (expected NXDOMAIN)
  ✗ doubleclick.net → NOERROR (expected NXDOMAIN)
  ✓ tracking.facebook.com → NXDOMAIN
  ✗ analytics.google.com → NOERROR (expected NXDOMAIN)
  ✗ ad.doubleclick.net → NOERROR (expected NXDOMAIN)
Testing allowed domains:
  ✓ google.com → NOERROR
  ✓ facebook.com → NOERROR
  ✓ github.com → NOERROR

Blocked: 1/5
Allowed: 3/3

=== Concurrency Test ===
Spawning 500 concurrent queries...
Completed 500 concurrent queries in .350161000s
Effective rate: 1427 qps

=== All Tests Complete ===

# 2

╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Fri Jan 23 19:22:26 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         80243
  Queries completed:    80243 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 80243 (100.00%)
  Average packet size:  request 28, response 49
  Run time (s):         10.010471
  Queries per second:   8015.906544

  Average Latency (s):  0.012366 (min 0.004823, max 0.113499)
  Latency StdDev (s):   0.001200


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 2.05ms
  P50: 2ms
  P95: 3ms
  P99: 4ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 960 cache hits
Cache hit rate: 96.00%
✓ 85% cache hit rate achieved

=== Blocklist Test ===
Testing blocked domains:
  ✗ ads.google.com → NOERROR (expected NXDOMAIN)
  ✗ doubleclick.net → NOERROR (expected NXDOMAIN)
  ✓ tracking.facebook.com → NXDOMAIN
  ✗ analytics.google.com → NOERROR (expected NXDOMAIN)
  ✗ ad.doubleclick.net → NOERROR (expected NXDOMAIN)
Testing allowed domains:
  ✓ google.com → NOERROR
  ✓ facebook.com → NOERROR
  ✓ github.com → NOERROR

Blocked: 1/5
Allowed: 3/3

=== Concurrency Test ===
Spawning 500 concurrent queries...
Completed 500 concurrent queries in .349017000s
Effective rate: 1432 qps

=== All Tests Complete ===

# 3

╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Fri Jan 23 20:18:11 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         172234
  Queries completed:    172234 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 172234 (100.00%)
  Average packet size:  request 28, response 49
  Run time (s):         10.005246
  Queries per second:   17214.369342

  Average Latency (s):  0.005727 (min 0.003637, max 0.037726)
  Latency StdDev (s):   0.000763


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 1.01ms
  P50: 1ms
  P95: 2ms
  P99: 3ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 990 cache hits
Cache hit rate: 99.00%
✓ 85% cache hit rate achieved

=== Blocklist Test ===
Testing blocked domains:
  ✓ ads.google.com → NXDOMAIN
  ✓ doubleclick.net → NXDOMAIN
  ✓ tracking.facebook.com → NXDOMAIN
  ✓ analytics.google.com → NXDOMAIN
  ✓ ad.doubleclick.net → NXDOMAIN
Testing allowed domains:
  ✓ google.com → NOERROR
  ✓ facebook.com → NOERROR
  ✓ github.com → NOERROR

Blocked: 5/5
Allowed: 3/3

=== Concurrency Test ===
Spawning 500 concurrent queries...
Completed 500 concurrent queries in .280359000s
Effective rate: 1783 qps

=== All Tests Complete ===
