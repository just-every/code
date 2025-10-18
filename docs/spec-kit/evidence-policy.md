# Evidence Repository Growth Policy

**Status**: v1.0 (2025-10-18)
**Owner**: theturtlecsz
**References**: REVIEW.md (architecture analysis), SPEC.md (DOC-4)

---

## 1. Overview

The spec-kit evidence repository stores telemetry, consensus artifacts, and validation results for all `/speckit.*` and `/guardrail.*` command executions. This document defines policies for managing evidence growth, retention, archival, and cleanup.

**Location**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`

**Structure**:
```
evidence/
├── commands/<SPEC-ID>/     # Guardrail telemetry JSON
├── consensus/<SPEC-ID>/    # Multi-agent consensus artifacts
└── .locks/<SPEC-ID>.lock   # File locks (ARCH-007)
```

---

## 2. Current State

**Total Size**: 38 MB (as of 2025-10-18)
**Growth Rate**: ~5-15 MB per full `/speckit.auto` run (depends on agent verbosity)
**Monitoring Tool**: `scripts/spec_ops_004/evidence_stats.sh`

**Usage Pattern**:
- **Commands**: 1-5 KB per stage (plan/tasks/implement/validate/audit/unlock)
- **Consensus**: 10-100 KB per agent × 3-5 agents = 30-500 KB per stage
- **Full Pipeline**: ~2-10 MB per SPEC (6 stages × 3-5 agents)

---

## 3. Size Limits

### 3.1 Soft Limits (Recommendations)

| Scope | Limit | Trigger Action |
|-------|-------|----------------|
| **Per-SPEC** | 25 MB | Review for cleanup (manual) |
| **Total Repository** | 500 MB | Archive old SPECs |
| **Per-File** | 5 MB | Investigate agent output verbosity |

**Rationale**:
- 25 MB per SPEC = ~5 full `/speckit.auto` runs with retries (sufficient for iteration)
- 500 MB total = ~20 active SPECs + historical data
- Git repositories become unwieldy >1 GB

### 3.2 Monitoring Commands

**Check total evidence size**:
```bash
du -sh docs/SPEC-OPS-004-integrated-coder-hooks/evidence/
```

**Check per-SPEC size**:
```bash
scripts/spec_ops_004/evidence_stats.sh --spec SPEC-KIT-065
```

**Check all SPECs**:
```bash
scripts/spec_ops_004/evidence_stats.sh
```

**Find large files**:
```bash
find docs/SPEC-OPS-004-integrated-coder-hooks/evidence/ -type f -size +2M -exec ls -lh {} \;
```

---

## 4. Retention Policy

### 4.1 Active SPECs

**Definition**: SPEC status ∈ {Backlog, In Progress, In Review, Blocked}

**Policy**: **KEEP ALL** evidence indefinitely
- Consensus artifacts may be needed for debugging
- Telemetry validates guardrail compliance
- Evidence enables retry analysis

**Exception**: If per-SPEC size exceeds 50 MB, compress consensus artifacts (see §5.2)

### 4.2 Completed SPECs

**Definition**: SPEC status = Done (unlocked + merged)

**Policy**: **KEEP for 30 days after unlock**, then archive or purge

**Rationale**:
- 30 days allows post-merge retrospective analysis
- Consensus artifacts rarely referenced after merge
- Telemetry serves as audit trail (can be offloaded)

**Action Timeline**:
1. SPEC unlocked → start 30-day retention clock
2. Day 30 → compress to `.tar.gz` (see §5.3)
3. Day 90 → offload to external storage (optional)
4. Day 180 → purge (if no archival value)

### 4.3 Abandoned SPECs

**Definition**: No activity for >90 days, never reached "Done"

**Policy**: **ARCHIVE immediately** (no retention period)

**Rationale**: Abandoned work unlikely to be revisited; free up space

---

## 5. Archival Strategy

### 5.1 When to Archive

**Triggers** (any of):
- Per-SPEC size exceeds 50 MB
- SPEC completed + 30 days elapsed
- SPEC abandoned (>90 days inactive)
- Total repository size approaches 500 MB

### 5.2 Compression (In-Place)

**Compress consensus artifacts** (largest files):
```bash
cd docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-065/
tar czf ../SPEC-KIT-065-consensus-archive.tar.gz *.json
rm *.json  # After verifying archive integrity
```

**Expected Compression**: 70-85% (JSON compresses well)

**Keep Uncompressed**:
- Latest consensus synthesis (`*_synthesis.json`)
- Telemetry JSONs (small, frequently accessed)

### 5.3 Offload (External Storage)

**Offload completed SPECs** to external storage after 90 days:

**Options**:
1. **Git LFS** (if repo supports)
2. **S3/Cloud Storage** (project-specific)
3. **Local archive directory** (outside repo)

**Process**:
```bash
# Create archive
tar czf SPEC-KIT-065-evidence-$(date +%Y%m%d).tar.gz \
  docs/SPEC-OPS-004-integrated-coder-hooks/evidence/{commands,consensus}/SPEC-KIT-065/

# Verify integrity
tar tzf SPEC-KIT-065-evidence-*.tar.gz | wc -l

# Move to external storage
mv SPEC-KIT-065-evidence-*.tar.gz /path/to/archive/

# Remove from repo
rm -rf docs/SPEC-OPS-004-integrated-coder-hooks/evidence/{commands,consensus}/SPEC-KIT-065/
```

**Metadata to Preserve**:
- SPEC ID, unlock date, final status
- Archive location and filename
- SHA256 checksum

---

## 6. Cleanup Procedures

### 6.1 Manual Cleanup (Safe)

**Identify candidates**:
```bash
# SPECs completed >30 days ago (check SPEC.md for unlock dates)
# SPECs abandoned (no consensus files newer than 90 days)
find docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/ \
  -type f -name "*_synthesis.json" -mtime +90
```

**Review before deletion**:
1. Check SPEC.md status (must be Done or abandoned)
2. Verify no active work references this SPEC
3. Confirm archive created (if valuable)

**Delete**:
```bash
rm -rf docs/SPEC-OPS-004-integrated-coder-hooks/evidence/{commands,consensus}/SPEC-KIT-065/
```

### 6.2 Automated Cleanup (Future)

**Not currently implemented**. Future automation could:
- Scan SPEC.md for completed SPECs
- Auto-compress after 30 days
- Auto-offload after 90 days
- Warn when total size >400 MB

**Implementation**: Consider adding to `/spec-evidence-stats` script or new `evidence_cleanup.sh`

---

## 7. Prevention Strategies

### 7.1 Reduce Evidence Size

**Agent Verbosity**:
- Agents sometimes include verbose reasoning (multi-KB outputs)
- Consider adding "Be concise" to agent prompts
- Or post-process consensus artifacts (extract structured data only)

**Retry Artifacts**:
- Retry attempts create duplicate artifacts
- Keep only successful consensus results (delete failed attempts after validation)

**Telemetry Efficiency**:
- Schema v1 telemetry is JSON (human-readable but large)
- Consider binary format (MessagePack, CBOR) for archival

### 7.2 Evidence Repository Rotation

**Not currently needed** (38 MB is manageable)

**Future Strategy** (if growth accelerates):
- Rotate evidence to quarterly archives: `evidence-2025Q4/`, `evidence-2026Q1/`
- Keep only current quarter + previous quarter in active repo
- Older quarters offloaded to external storage

---

## 8. Emergency Procedures

### 8.1 Repository Over Capacity

**Symptom**: Git operations slow, `git push` fails due to size

**Immediate Actions**:
1. Stop creating new evidence (`SPEC_OPS_ALLOW_DIRTY=1` for testing only)
2. Identify largest SPECs: `du -sh evidence/consensus/* | sort -h | tail -10`
3. Archive top 5 SPECs immediately (§5.3)
4. Resume operations once <400 MB

### 8.2 File System Errors

**Symptom**: `ENOSPC` (no space left), write failures

**Immediate Actions**:
1. Check disk space: `df -h`
2. Purge abandoned SPECs (no archive)
3. Compress all consensus artifacts (§5.2)
4. Escalate if still insufficient

---

## 9. Audit Trail

### 9.1 Required Metadata

When archiving or purging evidence, record:
- SPEC ID
- Action (compressed, archived, purged)
- Date
- Operator
- Archive location (if applicable)
- SHA256 checksum (if archived)

**Example Entry** (in SPEC.md Notes):
```
Evidence archived 2025-11-15: SPEC-KIT-065-evidence-20251115.tar.gz
SHA256: a3f2...b8c1, Location: /archive/2025Q4/, Operator: theturtlecsz
```

### 9.2 Recovery Procedures

**To restore archived evidence**:
```bash
# Extract archive to temporary location
tar xzf /archive/2025Q4/SPEC-KIT-065-evidence-20251115.tar.gz -C /tmp/

# Verify integrity
find /tmp/docs/ -type f -name "*.json" | wc -l  # Expected count

# Restore to repo
mv /tmp/docs/SPEC-OPS-004-integrated-coder-hooks/evidence/* \
   docs/SPEC-OPS-004-integrated-coder-hooks/evidence/
```

---

## 10. Review Cadence

**Quarterly Review**:
- Check total evidence size
- Identify SPECs eligible for archival
- Verify compression ratios
- Update this policy if growth patterns change

**Annual Review**:
- Evaluate archival strategy effectiveness
- Consider automated cleanup implementation
- Adjust size limits based on historical growth

**Next Review**: 2026-01-18 (Q1 2026)

---

## 11. Related Documentation

- `REVIEW.md`: Architecture analysis (unbounded growth identified)
- `SPEC.md`: Task DOC-4 (evidence policy creation)
- `ARCHITECTURE-TASKS.md`: ARCH-007 (evidence file locking)
- `scripts/spec_ops_004/evidence_stats.sh`: Monitoring tool
- `docs/spec-kit/evidence-baseline.md`: Baseline evidence structure

---

## 12. Change History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| v1.0 | 2025-10-18 | Initial policy | theturtlecsz |

---

## Appendix: Quick Reference

**Monitor evidence size**:
```bash
scripts/spec_ops_004/evidence_stats.sh [--spec SPEC-ID]
```

**Find SPECs over 25 MB**:
```bash
du -sh evidence/{commands,consensus}/* | awk '$1 ~ /M$/ && $1+0 > 25'
```

**Compress consensus for SPEC**:
```bash
cd evidence/consensus/SPEC-KIT-XXX/
tar czf ../SPEC-KIT-XXX-consensus.tar.gz *.json && rm *.json
```

**Archive completed SPEC**:
```bash
tar czf SPEC-KIT-XXX-$(date +%Y%m%d).tar.gz \
  evidence/{commands,consensus}/SPEC-KIT-XXX/
# Move to external storage, then rm -rf evidence/*/SPEC-KIT-XXX/
```
