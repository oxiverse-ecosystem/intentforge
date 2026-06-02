# Full gateway stress test: all endpoints, diverse queries, latency report
param([int]$DelayMs = 200)

$base = "http://localhost:4000"
$total = 0; $passed = 0; $failed = 0; $errors = @()
$results = [System.Collections.ArrayList]::new()

function Invoke-Test {
    param($endpoint, $query, $desc, [int]$expected_min = 0)
    $enc = [uri]::EscapeDataString($query)
    $url = "$base$endpoint`?q=$enc"
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $resp = Invoke-WebRequest -Uri $url -TimeoutSec 30 -UseBasicParsing
        $sw.Stop()
        $data = $resp.Content | ConvertFrom-Json
        $count = if ($data.results) { $data.results.Count } else { 0 }
        $intent = if ($data.intent) { $data.intent } else { "N/A" }
        $ok = $resp.StatusCode -eq 200 -and $count -ge $expected_min
        $script:total++
        if ($ok) { $script:passed++ } else { $script:failed++; $script:errors += "$desc`n  Status: $($resp.StatusCode) Results: $count Expected>= $expected_min`n  URL: $url" }
        [void]$script:results.Add([PSCustomObject]@{
            Endpoint = $endpoint; Query = $desc; Results = $count; Intent = $intent
            LatencyMs = [math]::Round($sw.Elapsed.TotalMilliseconds); Status = if($ok){"PASS"}else{"FAIL"}
        })
        Write-Output "$(if($ok){"PASS"}else{"FAIL"}) | $($sw.Elapsed.TotalSeconds.ToString('0.00'))s | $desc → $count results ($intent)"
    } catch {
        $sw.Stop()
        $script:total++; $script:failed++
        $err = $_.Exception.Message.Substring(0, [Math]::Min(100, $_.Exception.Message.Length))
        [void]$script:results.Add([PSCustomObject]@{
            Endpoint = $endpoint; Query = $desc; Results = -1; Intent = "ERR"
            LatencyMs = [math]::Round($sw.Elapsed.TotalMilliseconds); Status = "FAIL"
        })
        $script:errors += "$desc`n  $err`n  URL: $url"
        Write-Output "FAIL | $($sw.Elapsed.TotalSeconds.ToString('0.00'))s | $desc → ERROR: $err"
    }
}

Write-Output "`n═══════════════════════════════════════════════════════"
Write-Output "  INTENTFORGE v2 — FULL GATEWAY STRESS TEST"
Write-Output "  $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Output "═══════════════════════════════════════════════════════`n"

# ── /search (general) ──────────────────────────────────────────
Write-Output "── /search ──"
Invoke-Test "/search" "alternative to notion" "alternative to notion"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "best logging framework for rust microservices 2026" "rust logging framework comparison"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "how to deploy kubernetes on bare metal with ansible" "kubernetes bare metal ansible deploy"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "react server components vs traditional ssr performance" "react server components vs ssr"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "open source vector database comparison 2026" "vector database comparison"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "pytorch vs tensorflow for production inference latency benchmarks" "pytorch vs tensorflow inference"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "not django web framework" "not django"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "github actions self hosted runner setup with docker compose" "github actions self hosted runner"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "latest advances in large language model quantization 2026" "llm quantization advances"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search" "buy rust programming book online" "buy rust book"
Start-Sleep -Milliseconds $DelayMs

# ── /search/fast ─────────────────────────────────────────────
Write-Output "`n── /search/fast ──"
Invoke-Test "/search/fast" "rust tutorial" "rust tutorial fast"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search/fast" "docker compose networking" "docker compose networking fast"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search/fast" "python async await explained" "python async await fast"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search/fast" "postgresql connection pooling" "postgresql pooling fast"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/search/fast" "aws s3 vs cloudflare r2 pricing" "s3 vs r2 pricing fast"
Start-Sleep -Milliseconds $DelayMs

# ── /images ──────────────────────────────────────────────────
Write-Output "`n── /images ──"
Invoke-Test "/images" "rust programming language logo" "rust logo"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/images" "kubernetes architecture diagram" "k8s architecture"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/images" "microservices design pattern illustration" "microservices diagram"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/images" "neural network transformer architecture" "transformer architecture diagram"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/images" "docker vs vm comparison chart" "docker vs vm chart"
Start-Sleep -Milliseconds $DelayMs

# ── /videos ──────────────────────────────────────────────────
Write-Output "`n── /videos ──"
Invoke-Test "/videos" "rust programming tutorial for beginners" "rust tutorial video"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/videos" "kubernetes crash course 2026" "k8s crash course video"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/videos" "system design interview fundamentals" "system design video"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/videos" "machine learning pipeline deployment" "ml pipeline video"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/videos" "git merge vs rebase explained" "git merge rebase video"
Start-Sleep -Milliseconds $DelayMs

# ── /news ────────────────────────────────────────────────────
Write-Output "`n── /news ──"
Invoke-Test "/news" "rust language latest release" "rust release news"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/news" "kubernetes security vulnerability" "k8s security news"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/news" "openai latest model announcement" "openai news"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/news" "aws outage today" "aws outage news"
Start-Sleep -Milliseconds $DelayMs
Invoke-Test "/news" "webassembly browser support update" "wasm news"
Start-Sleep -Milliseconds $DelayMs

# ── Report ───────────────────────────────────────────────────
Write-Output "`n═══════════════════════════════════════════════════════"
Write-Output "  RESULTS"
Write-Output "═══════════════════════════════════════════════════════"
Write-Output "  Total:  $total  Passed: $passed  Failed: $failed"
$pct = if ($total -gt 0) { [math]::Round($passed/$total*100, 1) } else { 0 }
Write-Output "  Rate:   $pct%"
Write-Output ""

if ($results.Count -gt 0) {
    Write-Output "── Per-Endpoint Summary ──"
    $results | Group-Object Endpoint | ForEach-Object {
        $ep = $_.Name
        $n = $_.Count
        $avg = [math]::Round(($_.Group | Measure-Object LatencyMs -Average).Average)
        $max = ($_.Group | Measure-Object LatencyMs -Maximum).Maximum
        $pass = ($_.Group | Where-Object Status -eq 'PASS').Count
        Write-Output "  $ep  |  $n queries  |  ${pass}passed  |  avg ${avg}ms  |  max ${max}ms"
    }

    Write-Output "`n── Latency Distribution ──"
    $lats = $results.LatencyMs | Sort-Object
    $p50 = $lats[[math]::Floor($lats.Count*0.5)]
    $p90 = $lats[[math]::Floor($lats.Count*0.9)]
    $p95 = $lats[[math]::Floor($lats.Count*0.95)]
    $p99 = $lats[[math]::Floor($lats.Count*0.99)]
    Write-Output "  P50: ${p50}ms  P90: ${p90}ms  P95: ${p95}ms  P99: ${p99}ms"
    Write-Output "  Min: $($lats[0])ms  Max: $($lats[-1])ms  Avg: $([math]::Round(($lats | Measure-Object -Average).Average))ms"
}

if ($errors.Count -gt 0) {
    Write-Output "`n── Failures ($($errors.Count)) ──"
    $errors | ForEach-Object { Write-Output "  - $_" }
}

Write-Output "`n═══════════════════════════════════════════════════════`n"

# Return summary object
[PSCustomObject]@{
    Total = $total; Passed = $passed; Failed = $failed; Rate = "$pct%"
    AvgLatencyMs = if($results.Count -gt 0){[math]::Round(($results.LatencyMs | Measure-Object -Average).Average)}else{0}
    P95LatencyMs = $p95
}
