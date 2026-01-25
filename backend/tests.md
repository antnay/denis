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

# 4
╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Sat Jan 24 00:09:16 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         174861
  Queries completed:    174861 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 174861 (100.00%)
  Average packet size:  request 28, response 49
  Run time (s):         10.005121
  Queries per second:   17477.149952

  Average Latency (s):  0.005651 (min 0.001826, max 0.094054)
  Latency StdDev (s):   0.000940


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 0.92ms
  P50: 1ms
  P95: 1ms
  P99: 2ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 998 cache hits
Cache hit rate: 99.80%
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
Completed 500 concurrent queries in .255304000s
Effective rate: 1958 qps

=== All Tests Complete ===

# 5 1e06c3dcee11a9bd79222a1f432d8f11e7eac6b9
denis/backend main ⇡❯ ./stress_test.sh
╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Sat Jan 24 19:56:09 2026
[Status] Stopping after 10.000000 seconds
[Timeout] Query timed out: msg id 134
[Timeout] Query timed out: msg id 137
[Timeout] Query timed out: msg id 140
[Timeout] Query timed out: msg id 143
Warning: received a response with an unexpected (maybe timed out) id: 134
Warning: received a response with an unexpected (maybe timed out) id: 137
Warning: received a response with an unexpected (maybe timed out) id: 140
Warning: received a response with an unexpected (maybe timed out) id: 143
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         174500
  Queries completed:    174496 (100.00%)
  Queries lost:         4 (0.00%)

  Response codes:       NOERROR 174496 (100.00%)
  Average packet size:  request 28, response 49
  Run time (s):         10.005667
  Queries per second:   17439.716912

  Average Latency (s):  0.005542 (min 0.000970, max 0.088850)
  Latency StdDev (s):   0.000913


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 1.98ms
  P50: 2ms
  P95: 3ms
  P99: 4ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 996 cache hits
Cache hit rate: 99.60%
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
Completed 500 concurrent queries in .304634000s
Effective rate: 1641 qps

=== All Tests Complete ===

# 6
╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Sat Jan 24 21:24:55 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         173996
  Queries completed:    173996 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 173996 (100.00%)
  Average packet size:  request 28, response 32
  Run time (s):         10.005363
  Queries per second:   17390.273596

  Average Latency (s):  0.005681 (min 0.001285, max 0.075767)
  Latency StdDev (s):   0.001460


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 1.93ms
  P50: 2ms
  P95: 3ms
  P99: 3ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 980 cache hits
Cache hit rate: 98.00%
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
Completed 500 concurrent queries in .326668000s
Effective rate: 1530 qps

=== All Tests Complete ===

# 7 cache add is not on new thread
╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Sat Jan 24 21:27:32 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         174409
  Queries completed:    174409 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 174409 (100.00%)
  Average packet size:  request 28, response 28
  Run time (s):         10.005211
  Queries per second:   17431.816281

  Average Latency (s):  0.005668 (min 0.000633, max 0.054283)
  Latency StdDev (s):   0.001109


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 1.93ms
  P50: 2ms
  P95: 3ms
  P99: 3ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 996 cache hits
Cache hit rate: 99.60%
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
Completed 500 concurrent queries in .294854000s
Effective rate: 1695 qps

=== All Tests Complete ===

# 8 buffer pool

╔════════════════════════════════════════╗
║     DNS Analytics Proxy Stress Test    ║
║     Server: 127.0.0.1:5354               ║
╚════════════════════════════════════════╝

=== Throughput Test (Target: 50k qps) ===
DNS Performance Testing Tool
Version 2.14.0

[Status] Command line: dnsperf -s 127.0.0.1 -p 5354 -d /tmp/queries.txt -l 10 -c 100 -Q 60000
[Status] Sending queries (to 127.0.0.1:5354)
[Status] Started at: Sun Jan 25 02:01:14 2026
[Status] Stopping after 10.000000 seconds
[Status] Testing complete (time limit)

Statistics:

  Queries sent:         173155
  Queries completed:    173155 (100.00%)
  Queries lost:         0 (0.00%)

  Response codes:       NOERROR 173155 (100.00%)
  Average packet size:  request 28, response 49
  Run time (s):         10.005164
  Queries per second:   17306.562891

  Average Latency (s):  0.005707 (min 0.000837, max 0.148828)
  Latency StdDev (s):   0.002623


=== Latency Test (Target: <1ms p99) ===
1000 queries completed
  Avg: 0.91ms
  P50: 1ms
  P95: 1ms
  P99: 2ms
✗ P99 above 1ms

=== Cache Hit Rate Test (Target: 85%) ===
1000 queries, 999 cache hits
Cache hit rate: 99.90%
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
Completed 500 concurrent queries in .284895000s
Effective rate: 1755 qps

=== All Tests Complete ===
