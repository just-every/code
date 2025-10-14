# Spec-Kit Automation Framework - Product Requirements

> Status: v1.0 (2025-10-14) — Corrected scope: spec-kit framework, not Kavedarr product

## 1. Product Summary
- **Product name:** Spec-Kit Automation Framework
- **Domain:** Multi-agent development workflow automation
- **Mission:** Enable AI-driven feature development through consensus-based planning, implementation, and validation with full evidence tracking

## 2. Primary Users & Goals
- **Software Development Teams** – Automate feature development from idea to implementation using multi-model AI consensus
- **Engineering Leads** – Track development progress with auditable evidence trails and multi-agent validation
- **DevOps/Platform Engineers** – Integrate spec-kit workflows into CI/CD pipelines with telemetry-driven gating

## 3. What Spec-Kit Provides

**Core Workflow:**
```
/new-spec <description>
  → Multi-agent PRD creation
  → SPEC-ID generation

/spec-auto SPEC-ID
  → Plan (multi-agent work breakdown)
  → Tasks (multi-agent task decomposition)
  → Implement (code generation with consensus)
  → Validate (test execution)
  → Audit (compliance review)
  → Unlock (approval for merge)
```

**Key Features:**
- **Multi-model consensus:** Gemini (research), Claude (synthesis), GPT-5 (validation), GPT-5-Codex (code generation)
- **Automatic conflict resolution:** Arbiter agent resolves disagreements without human intervention
- **Evidence-driven:** Every stage produces telemetry, consensus synthesis, audit trails
- **Visible execution:** All agent work shown in TUI, no black boxes
- **Progressive validation:** Each stage validates previous work before advancing

## 4. Current Capabilities (October 2025)

**Automation:**
- ✅ `/new-spec` - Creates SPEC from natural language description
- ✅ `/spec-auto` - Executes 6-stage pipeline with multi-agent consensus
- ✅ Automatic stage advancement (no manual gates)
- ✅ Conflict resolution via arbiter agents
- ✅ Parallel agent spawning (faster execution)

**Guardrails:**
- ✅ Baseline audits per stage
- ✅ Policy compliance checks (constitution, model strategy)
- ✅ HAL validation (mock mode, optional live mode)
- ✅ Schema v1 telemetry with evidence artifacts

**Diagnostics:**
- ✅ `/spec-status` - Dashboard showing progress across all stages
- ✅ Agent execution logging and analysis
- ✅ Evidence footprint monitoring

**Integration:**
- ✅ Codex CLI/TUI native commands
- ✅ MCP server ecosystem (local-memory, repo-search, doc-index, hal)
- ✅ Git-based evidence storage

## 5. Functional Requirements

**FR-1: Multi-Agent Consensus**
- Each stage spawns 3-5 agents (gemini, claude, gpt_pro, gpt_codex, code)
- Agents work in parallel where possible
- Consensus synthesis identifies agreements and conflicts
- Arbiter resolves conflicts automatically
- Only halt on true deadlocks (rare)

**FR-2: Evidence Tracking**
- Every stage produces JSON telemetry (schema v1)
- Consensus synthesis stored per stage
- Agent outputs preserved for audit
- Telemetry includes model metadata, reasoning modes, timestamps

**FR-3: Progressive Validation**
- Plan validates PRD completeness
- Tasks validates plan coverage
- Implement validates task completion
- Validate runs tests
- Audit checks compliance
- Unlock requires all prior stages passed

**FR-4: Visible Execution**
- Bash guardrails show output in TUI
- Agent spawning visible
- Progress indicators (Stage X/6)
- Errors surface immediately
- Can interrupt execution

**FR-5: File Deliverables**
- Plan stage: Creates plan.md (work breakdown)
- Tasks stage: Creates tasks.md (task list)
- Implement stage: Creates code, tests, docs
- Validate/Audit/Unlock: Creates reports, evidence

## 6. Non-Functional Requirements

**Performance:**
- Full 6-stage pipeline: 40-60 minutes
- Context caching reduces redundant file reads
- Parallel agent spawning (30% faster than sequential)

**Reliability:**
- Fails fast on dirty git tree
- Policy checks enforce constitution compliance
- Agent failures handled gracefully (degraded mode)

**Auditability:**
- Timestamped telemetry per stage
- Consensus synthesis with conflict resolution
- Evidence organized by SPEC-ID

**Security:**
- No secrets in git
- HAL credentials via environment variables
- Sandbox isolation for agent execution

## 7. Technology Stack

**Core:**
- Rust (Codex CLI fork - anthropics/claude-code upstream)
- Bash (guardrail scripts)
- Python (telemetry utilities)

**AI Models:**
- Gemini 2.5 Pro (research, breadth)
- Claude 4.5 Sonnet (synthesis, precision)
- GPT-5 (validation, arbitration)
- GPT-5-Codex (code generation)

**Infrastructure:**
- MCP servers (local-memory, repo-search, doc-index, hal)
- Git-based evidence storage
- TUI-native commands

## 8. Example Use Cases

**Kavedarr (Media Automation):**
- Uses spec-kit to develop media asset management features
- HAL MCP validates Kavedarr API endpoints
- Example SPEC: SPEC-KIT-DEMO

**Other Projects:**
- Any software project can use spec-kit
- Framework is project-agnostic
- Customize prompts, guardrails, validation per project

## 9. Success Metrics

**Automation:**
- Idea → implementation without manual intervention ✅
- Multi-agent consensus reaches resolution 95%+ of time
- <5% deadlocks requiring human input

**Speed:**
- Full pipeline: 40-60 minutes (down from 96 min with optimizations)
- Single stage: 6-10 minutes

**Quality:**
- Multi-model perspectives catch gaps single agent would miss
- Evidence trails enable debugging and accountability
- Constitution compliance enforced automatically

## 10. Open Questions

- Default HAL mode: mock or live?
- Evidence archival when exceeding 25MB per SPEC
- Cost tracking for high-reasoning multi-agent runs
- Extract spec-kit to separate repo or keep embedded?

## 11. Review Notes

**Scope correction:** 2025-10-14
- Previous versions incorrectly described Kavedarr product
- Corrected to describe spec-kit automation framework
- Kavedarr is an example use case, not the product itself

**Status:** v1.0 (spec-kit framework correctly scoped)
**Owner:** @just-every/automation
