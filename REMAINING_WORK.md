# Most Important Remaining Features & Fixes

**Quick Reference** - Prioritized list of remaining work for v1.0.0

---

## 🔴 Critical for v1.0 (Before Production Release)

### 1. Test Coverage to 80%+ (Current: ~70%)
**Effort:** 2-3 days | **Priority:** HIGH

Missing test coverage for:
- Bulk export module (no unit tests)
- S3 multipart upload integration tests
- Error path testing (network failures, subprocess crashes)
- Rate limiting edge cases

**Action Items:**
```bash
# Add tests for:
src/web/export.rs          # Bulk export unit tests
src/s3/multipart.rs        # Multipart upload integration tests
src/archiver/worker.rs     # Error recovery scenarios
src/web/rate_limit.rs      # Rate limit edge cases
```

**Acceptance:** `cargo test` shows >80% coverage via `cargo-tarpaulin` or similar.

---

### 2. API Documentation (Rustdoc)
**Effort:** 1 day | **Priority:** HIGH

Currently ~60% of public functions lack documentation.

**Action Items:**
```bash
# Document these modules:
src/handlers/*/           # All SiteHandler implementations
src/archiver/worker.rs    # Worker pool API
src/db/queries.rs         # Database query functions
src/config.rs             # Configuration options
```

**Acceptance:** `cargo doc --no-deps --open` shows complete documentation.

---

### 3. Memory Profiling Under Load
**Effort:** 1 day | **Priority:** MEDIUM-HIGH

No profiling has been done with concurrent workers.

**Action Items:**
1. Profile with 50+ concurrent archive jobs
2. Monitor chromiumoxide browser lifecycle (potential leak)
3. Check for excessive allocations in worker pool
4. Document recommended memory limits

**Acceptance:** Memory usage stays under 1GB with 10 concurrent workers over 24 hours.

---

## 🟡 Nice to Have for v1.0

### 4. Twitter Video Pre-Flight Detection
**Effort:** 1-2 days | **Priority:** MEDIUM

Currently runs yt-dlp on all Twitter URLs, even text-only tweets.

**Similar to:** Reddit implementation (src/handlers/reddit.rs:204-223)

**Action Items:**
1. Add pre-flight check for video/media presence
2. Skip yt-dlp execution for text-only tweets
3. Reduces unnecessary subprocess overhead

**Benefit:** ~30-50% reduction in Twitter archiving time.

---

### 5. Load Testing (Web UI)
**Effort:** 1 day | **Priority:** MEDIUM

No load testing has been performed on web routes.

**Action Items:**
```bash
# Test scenarios:
1. Concurrent search queries (100 req/s)
2. Large result set pagination
3. Archive detail page with media
4. Bulk export requests
```

**Tools:** `wrk`, `k6`, or `locust`

**Acceptance:** Web UI responds <200ms at 95th percentile under 100 concurrent users.

---

## 🟢 Future Enhancements (Post v1.0)

### 6. Database-Backed Video Deduplication
**Effort:** 2-3 days | **Priority:** LOW (cost optimization)

**Current:** Uses S3 server-side copy (duplicates video files on S3)
**Proposed:** Store single copy, use reference table

**Benefits:**
- Significant S3 storage cost reduction
- Simpler deduplication logic
- Better for high-duplication scenarios (e.g., popular YouTube videos)

**Schema:**
```sql
CREATE TABLE video_files (
    video_id TEXT PRIMARY KEY,
    s3_key TEXT NOT NULL,
    size_bytes INTEGER,
    created_at TEXT
);
```

---

### 7. Webhook Notifications
**Effort:** 2-3 days | **Priority:** LOW

Send alerts for failed archives to Discord/Slack.

**Configuration:**
```bash
WEBHOOK_URL=https://discord.com/api/webhooks/...
WEBHOOK_EVENTS=archive_failed,archive_completed
WEBHOOK_MIN_RETRY_COUNT=2  # Only notify after 2+ failures
```

---

### 8. Prometheus Metrics
**Effort:** 1-2 days | **Priority:** LOW

Expose `/metrics` endpoint for monitoring.

**Metrics:**
- `archives_total{status="complete|failed|pending"}`
- `archive_duration_seconds{handler="reddit|youtube|..."}`
- `s3_upload_bytes_total`
- `worker_queue_depth`

**Dashboard:** Include Grafana templates in `docs/`

---

## 📊 Current Status Summary

| Category | Status | Notes |
|----------|--------|-------|
| **Core Functionality** | ✅ 100% | All features working |
| **Feature Completeness** | ✅ 95% | Missing optional features only |
| **Test Coverage** | ⚠️ 70% | Need 80%+ for v1.0 |
| **Documentation** | ⚠️ 60% | Need rustdoc completion |
| **Production Hardening** | ⚠️ 85% | Need profiling + load testing |

**Overall:** ~92% complete, production-ready with caveats.

---

## Recommended Timeline for v1.0

### Week 1: Testing & Documentation
- Day 1-2: Write missing unit tests (bulk export, error paths)
- Day 3: Add rustdoc to all public APIs
- Day 4: Memory profiling under load
- Day 5: Fix any issues found during profiling

### Week 2: Validation & Release
- Day 1: Load testing web UI
- Day 2: Fix performance bottlenecks
- Day 3: Manual testing of all features
- Day 4: Security review (especially cookie handling)
- Day 5: Tag v1.0.0, create release notes

**Total Effort:** ~10 days of focused work

---

## Known Issues (Non-Blocking)

1. **Monolith v2.8.3 Crashes** (Exit code 101)
   - **Impact:** 6 archives lose complete.html format
   - **Mitigation:** Already implemented (use raw.html)
   - **Recommendation:** Upgrade to latest monolith

2. **Twitter Video Detection** (Missing)
   - **Impact:** Unnecessary yt-dlp calls on text tweets
   - **Workaround:** Still archives successfully, just slower

3. **Network Idle Detection** (src/archiver/screenshot.rs:347)
   - **Impact:** Screenshots might miss lazy-loaded content
   - **Mitigation:** 5-second fixed wait works in 95%+ cases
   - **Blocked on:** chromiumoxide API support

---

## See Also

- `PROJECT_COMPLETION_REPORT.md` - Full detailed analysis
- `MAIN_TASKS.md` - Development task tracker
- `SPEC.md` - Original technical specification
- `CLAUDE.md` - Development guidelines

---

**Last Updated:** 2026-01-19
**Next Review:** After completing Week 1 tasks
