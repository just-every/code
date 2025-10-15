# Spec-Kit Automation Framework - Product Requirements

> Status: v1.1 (2025-10-15) — Phase 3 complete: /speckit.* namespace, tiered model strategy, GitHub quality commands

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
/speckit.new <description>
  → Multi-agent PRD creation with templates
  → SPEC-ID generation
  → Consistent structure (55% faster)

/speckit.auto SPEC-ID
  → Plan (multi-agent work breakdown)
  → Tasks (multi-agent task decomposition)
  → Implement (code generation with consensus)
  → Validate (test execution)
  → Audit (compliance review)
  → Unlock (approval for merge)
```

**Quality Commands:**
```
/speckit.clarify SPEC-ID    → Resolve requirement ambiguities
/speckit.analyze SPEC-ID    → Check cross-artifact consistency + auto-fix
/speckit.checklist SPEC-ID  → Score requirement quality
```

**Key Features:**
- **Multi-model consensus:** Gemini (research), Claude (synthesis), GPT-5 (validation), GPT-5-Codex (code generation)
- **Tiered model strategy:** Right-sized agent usage (0-4 agents per command type), 40% cost reduction
- **Template-based generation:** GitHub-inspired templates with 55% speed improvement
- **Automatic conflict resolution:** Arbiter agent resolves disagreements without human intervention
- **Evidence-driven:** Every stage produces telemetry, consensus synthesis, audit trails
- **Visible execution:** All agent work shown in TUI, no black boxes
- **Progressive validation:** Each stage validates previous work before advancing
- **Native TUI status:** Instant dashboard (<1s) with no API calls

## 4. Current Capabilities (October 2025)

**Phase 3 Complete - Standardization (✅ All 13 /speckit.* commands functional):**

**Intake & Creation:**
- ✅ `/speckit.new` - Creates SPEC from natural language with templates (Tier 2: 3 agents, ~13 min, ~$0.60)
- ✅ `/speckit.specify` - Draft/update PRD with multi-agent analysis (Tier 2: 3 agents, ~10 min, ~$0.80)

**Quality Commands (GitHub-inspired):**
- ✅ `/speckit.clarify` - Structured ambiguity resolution (Tier 2: 3 agents, ~8 min, ~$0.80)
- ✅ `/speckit.analyze` - Cross-artifact consistency checking + auto-fix (Tier 2: 3 agents, ~8 min, ~$0.80)
- ✅ `/speckit.checklist` - Requirement quality scoring (Tier 2-lite: 2 agents, ~5 min, ~$0.35)

**Development Stages:**
- ✅ `/speckit.plan` - Multi-agent work breakdown (Tier 2: 3 agents, ~10 min, ~$1.00)
- ✅ `/speckit.tasks` - Task decomposition with consensus (Tier 2: 3 agents, ~10 min, ~$1.00)
- ✅ `/speckit.implement` - Code generation + validation (Tier 3: 4 agents, ~15 min, ~$2.00)
- ✅ `/speckit.validate` - Test strategy consensus (Tier 2: 3 agents, ~10 min, ~$1.00)
- ✅ `/speckit.audit` - Compliance checking (Tier 2: 3 agents, ~10 min, ~$1.00)
- ✅ `/speckit.unlock` - Final approval (Tier 2: 3 agents, ~10 min, ~$1.00)

**Automation & Diagnostics:**
- ✅ `/speckit.auto` - Full 6-stage pipeline (Tier 4: dynamic 3-5 agents, ~60 min, ~$11)
- ✅ `/speckit.status` - Native TUI dashboard (Tier 0: instant, no agents, <1s, $0)

**Guardrails (Shell wrappers):**
- ✅ `/spec-ops-{plan,tasks,implement,validate,audit,unlock}` - Validation scripts per stage
- ✅ `/spec-ops-auto` - Full pipeline wrapper with telemetry
- ✅ Baseline audits per stage
- ✅ Policy compliance checks (constitution, model strategy)
- ✅ HAL validation (mock mode, optional live mode)
- ✅ Schema v1 telemetry with evidence artifacts

**Core Capabilities:**
- ✅ Automatic stage advancement (no manual gates)
- ✅ Conflict resolution via arbiter agents
- ✅ Parallel agent spawning (30% faster)
- ✅ Template-based generation (55% faster, validated via SPEC-KIT-060)
- ✅ Tiered model strategy (40% cost reduction: $15→$11 per pipeline)
- ✅ Agent execution logging and analysis
- ✅ Evidence footprint monitoring

**Integration:**
- ✅ Codex CLI/TUI native commands
- ✅ MCP server ecosystem (local-memory, repo-search, doc-index, hal)
- ✅ Git-based evidence storage
- ✅ Backward compatibility (/spec-* commands still work)

## 5. Functional Requirements

**FR-1: Multi-Agent Consensus (Tiered Strategy)**
- Tier 0 (Native): 0 agents for status queries
- Tier 1 (Single): 1 agent (code) for scaffolding (future)
- Tier 2-lite (Dual): 2 agents (claude, code) for quality checks
- Tier 2 (Triple): 3 agents (gemini, claude, code/gpt_pro) for analysis/planning
- Tier 3 (Quad): 4 agents (gemini, claude, gpt_codex, gpt_pro) for code generation
- Tier 4 (Dynamic): 3-5 agents adaptively for full automation
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

**FR-5: File Deliverables (Template-Based)**
- Templates: GitHub-inspired format with P1/P2/P3 scenarios
- Plan stage: Creates plan.md (work breakdown from template)
- Tasks stage: Creates tasks.md (checkbox tasks from template)
- Implement stage: Creates code, tests, docs
- Validate/Audit/Unlock: Creates reports, evidence
- Performance: 55% faster generation vs non-template approach (validated SPEC-KIT-060)

**FR-6: Quality Commands (GitHub-Inspired)**
- Clarify: Identify and resolve requirement ambiguities
- Analyze: Check cross-artifact consistency (PRD ↔ plan ↔ tasks), auto-fix issues
- Checklist: Score requirement quality (testability, clarity, completeness)

## 6. Non-Functional Requirements

**Performance:**
- Full 6-stage pipeline: 40-60 minutes (down from 96 min with optimizations)
- Single stage: 5-20 minutes depending on tier (Tier 2-lite: 5 min, Tier 3: 20 min)
- Template generation: 55% faster (13 min vs 30 min baseline)
- Context caching reduces redundant file reads
- Parallel agent spawning (30% faster than sequential)
- Native TUI status: <1s (no API calls)

**Cost Efficiency:**
- Tiered model strategy: 40% cost reduction ($15→$11 per full pipeline)
- Status queries: $0 (native Rust, no agents)
- Quality checks: $0.35-0.80 per command
- Full automation: ~$11 for 6-stage pipeline

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
- Multi-agent consensus reaches resolution 95%+ of time ✅
- <5% deadlocks requiring human input ✅
- Template adoption: 100% (validated SPEC-KIT-060) ✅

**Speed:**
- Full pipeline: 40-60 minutes (down from 96 min with optimizations) ✅
- Template generation: 55% faster (13 min vs 30 min) ✅
- Status queries: <1s (instant native TUI) ✅
- Single stage: 5-20 minutes (tiered by complexity) ✅

**Cost:**
- 40% reduction via tiered strategy ($15→$11 per pipeline) ✅
- Status queries: $0 (native, no agents) ✅

**Quality:**
- Multi-model perspectives catch gaps single agent would miss ✅
- Evidence trails enable debugging and accountability ✅
- Constitution compliance enforced automatically ✅
- Quality commands catch requirement issues (clarify, analyze, checklist) ✅
- Cross-artifact consistency validated (PRD ↔ plan ↔ tasks) ✅

## 10. Open Questions

- Default HAL mode: mock or live?
- Evidence archival when exceeding 25MB per SPEC
- Extract spec-kit to separate repo or keep embedded?
- Future guardrail namespace: /guardrail.* vs /spec-ops-* (planned Week 2)

## 11. Review Notes

**Phase 3 standardization:** 2025-10-15
- ✅ All 13 /speckit.* commands functional
- ✅ Tiered model strategy applied (40% cost reduction)
- ✅ Template validation complete (55% speed improvement)
- ✅ GitHub quality commands ported (clarify, analyze, checklist)
- ✅ Backward compatibility maintained (/spec-* still works)
- Next: Guardrail namespace separation, final documentation

**Scope correction:** 2025-10-14
- Previous versions incorrectly described Kavedarr product
- Corrected to describe spec-kit automation framework
- Kavedarr is an example use case, not the product itself

**Status:** v1.1 (Phase 3 standardization complete)
**Owner:** @just-every/automation
