# Strata (formerly Prism, ignore henceforth) 
## AI-Native Behavioral Talent Intelligence Platform
### Product Spec v1.1 — March 2026

---

## 1. Executive Summary

Strata is a **skills infrastructure layer** for the AI tool ecosystem — with a talent marketplace built on top.

A local MCP server runs on the user's machine and serves structured skill context to any MCP-compatible AI tool (Claude, Cursor, Copilot, Gemini, Windsurf, and any future tool). Those tools get smarter in every session by knowing who they're talking to. As a side effect, every tool query passively enriches a longitudinal behavioral profile. That profile — compressed, user-permissioned, cryptographically attested, and raw-data-free — becomes the basis for a new kind of hiring signal: behavioral evidence of how someone actually thinks and works with AI.

**The product hierarchy:**
1. Skills infrastructure (MCP server) — daily active utility, every AI tool session
2. Personal skill profile — longitudinal record of growth and work style
3. Talent marketplace — employers discover candidates by behavioral evidence, not credentials

**The core safety commitment:** Raw activity data never leaves the user's device. Ever. What employers see is a cryptographically attested, semantically compressed intelligence profile. This is not a privacy feature. It is the foundational architecture.

---

## 2. The Strategic Reframe

The original framing was: *a talent platform with a local agent.*

The correct framing is: *a skills graph that the AI tool ecosystem queries daily, with a talent marketplace on top.*

This distinction matters for everything — product design, go-to-market, moat, and investor narrative.

| | Talent Platform w/ Agent | Skills Infrastructure w/ Marketplace |
|---|---|---|
| Primary daily value | Occasional (job seeking) | Constant (every tool session) |
| Data collection model | Passive observation | Active tool queries (pull, not push) |
| Churn incentive | Low profile investment | Leaving makes your AI tools dumber |
| Privacy optics | "Surveillance" risk | Explicit scoped requests — clean |
| Competitive moat | Profile mass | Tool ecosystem integration depth |
| Network effects | Candidate ↔ employer | Tools ↔ users ↔ employers (3-sided) |

---

## 3. Problem Statement

### For Candidates
- Résumés are static, self-reported, and indistinguishable from fabrication
- AI skills are the fastest-growing job requirement — 275,000+ active postings reference AI skills (Jan 2026), demand grew 7x from 2023–2025 — yet no trusted behavioral signal exists for demonstrating them
- AI tools have no persistent knowledge of who they're working with, so every session starts cold

### For Employers
- AI talent demand exceeds supply 3.2:1 globally; 1.6M open positions, ~518K qualified candidates
- Point-in-time assessments are gameable and don't predict ongoing performance
- Eightfold AI faced a class action FCRA lawsuit in January 2026 for compiling and scoring candidate profiles without consent — the entire competitive set is building toward this legal exposure
- AI-exposed role skill requirements evolve 66% faster than non-exposed roles — any static profile is already stale

### For AI Tools
- Every tool session starts without knowledge of the user's skill level, preferences, or working style
- Tools give the same explanation to a senior ML engineer and a first-week bootcamp grad
- No standard exists for tools to share or query user skill context

### The Gap
No platform serves all three sides simultaneously. Prism is the connective tissue between users, tools, and employers — with privacy as the architectural foundation that makes all three sides trust it.

---

## 4. Market Opportunity

| Segment | Size | Source |
|---|---|---|
| US tech labor force | ~9.8M workers (2026) | CompTIA |
| Active AI-skill job postings | 275,000+ (Jan 2026) | CompTIA |
| Global AI talent shortage | 1.6M open positions | Second Talent |
| LinkedIn Talent Solutions revenue | $7B+ (2023) | Public |
| Total LinkedIn revenue | $14B+ (2024) | Public |
| US job board market | $14.7B | Industry |
| AI role salary premium | 56–67% above market | PwC / Second Talent |

**SAM:** Recruiter tooling alone exceeds $5B annually. MCP integration creates a second TAM — tool personalization infrastructure — that doesn't exist yet as a standalone market but is latent in every AI tool's need for persistent user context.

**Why now:** MCP has become the de facto standard for AI tool integrations. The ecosystem is standardizing on it in real time. Prism can be the canonical skills context provider for that ecosystem before anyone else claims the position.

---

## 5. Privacy & Safety Architecture

> **This section precedes product description intentionally. Privacy is not a setting. It is the reason all three sides of the market trust Prism.**

### 5.1 The Fundamental Principle: Local-First, Derive-Then-Discard

Raw activity data — prompts, responses, tool sessions, document content, MCP query logs — is processed entirely on-device. It never reaches Prism's servers. What transmits upstream is only derived semantic signal, not source material.

```
ON DEVICE                               PRISM SERVERS
──────────────────────────────────      ────────────────────────
Raw session / MCP query log
  ↓ (local LLM inference)
Skill extraction
  ↓ (local aggregation)
Semantic signal vector  ─────────────→  Encrypted skill vectors
  ↓ (local purge)                       Role-fit scores
Raw data deleted                        Work style tags
                                        Attestation record
```

### 5.2 On-Device Inference Engine

The local agent uses a quantized small-parameter LLM (Phi-4-mini or Llama 3.2 3B class) running via Ollama, embedded in the Prism desktop client. All raw extraction is local. The model performs three tasks:

1. **Skill tagging** — structured enum tags from ESCO + AI-native taxonomy extension
2. **Outcome summarization** — max 200 characters, PII-scanned before any output
3. **AI collaboration classification** — categorical label (Level 1–5)

The model does not store prompts, completions, document content, or any identifiable text.

### 5.3 MCP Query Privacy

MCP query logs — which tool called which endpoint, how often, when — are stored locally only and feed the skill graph as passive signals. They are never transmitted to Prism servers. An employer cannot see that "Cursor called `prism://skills/kubernetes` 14 times this week." They see only the resulting skill vector score. Query activity is signal fuel, not an exposed data surface.

### 5.4 Differential Privacy on Signal Upload

Before any derived signal leaves the device, a DP layer adds calibrated Laplacian noise (Apple/Google open-source DP library). Individual activity events cannot be reverse-engineered from transmitted data. The privacy budget (ε) is user-configurable:

- **Low ε (0.1–0.5):** Maximum privacy, higher noise, less precise profile
- **Medium ε (0.6–1.2):** Default — strong event protection, good signal quality
- **High ε (1.3–2.0):** Maximum precision, reduced individual event protection

### 5.5 Zero Raw Snapshots — Architectural Guarantee

Prism's API gateway is schema-enforced to be structurally incapable of ingesting raw text. The signal ingestion endpoint accepts only:

- Float arrays (skill vectors)
- Enum-only categorical fields
- Strings ≤ 200 characters (PII-screened before transmission)
- Week-granularity timestamps (not precise)

This is not a policy constraint. It is a schema constraint auditable by third-party security review.

### 5.6 Employer-Visible Data

| Data Type | On Device | Transmitted | Employer Visible |
|---|---|---|---|
| Raw prompts/responses | Transient → purged | ❌ Never | ❌ Never |
| Document/code content | Transient → purged | ❌ Never | ❌ Never |
| MCP query logs | Local only | ❌ Never | ❌ Never |
| Derived skill vectors | Aggregated locally | ✅ DP-noised | ✅ User consent |
| Role-fit scores | Computed server-side | N/A | ✅ User consent |
| Work style narrative | Generated from vectors | N/A | ✅ User consent |
| Outcome summaries | User-reviewed | ✅ User-approved | ✅ Opt-in per item |

### 5.7 Three-Layer Consent Model

**Layer 1 — Capture + MCP Consent (per-tool, per-context, default: off for passive capture / on for MCP read)**

The MCP server defaults to on for read-only endpoints once the agent is installed. This is the correct default — read queries make tools immediately better, driving adoption. Write endpoints (`prism://signal`) require explicit user enable. Passive capture defaults to off; user enables per connector.

Per-tool permission matrix controls which endpoints each tool can call. Users configure this in agent settings.

**Layer 2 — Signal Transmission Consent (weekly digest, user-approved)**

Weekly digest surfaces extracted signals. User reviews, edits, approves, or discards per item. Nothing transmits without explicit approval. Discarded items are deleted locally.

**Layer 3 — Employer Visibility Consent (per-employer, 30-day tokens)**

Individual access tokens scoped by employer ID, profile section, and expiry date. Revocation cascades: token invalidated immediately, employer-cached data purged within 24 hours.

### 5.8 Attestation and Trust

Employers will ask: *how do we know the profile is real?*

Each signal upload is signed with a device-bound key (TPM 2.0 where available, secure enclave otherwise). Prism's server verifies the signature and records that the signal originated from a consistent device identity over time. Long-term consistency scoring makes fabrication detectable — you cannot fake a 14-month skill trajectory on a consistent hardware identity. Optional corroborating signals: verified GitHub commit history, Notion workspace activity — not raw data, temporal correlation proofs only.

### 5.9 Regulatory Compliance by Design

| Regulation | How Prism Complies |
|---|---|
| GDPR / EU AI Act (employment = high-risk) | Local-first processing avoids most high-risk triggers. User-permissioned model satisfies consent. DPIA template provided at onboarding. Human oversight on all employment-affecting decisions. |
| EU AI Act Article 5 (prohibited uses) | No emotion recognition. No social scoring. No covert surveillance. |
| FCRA (US) | All data user-originated and user-attested. Not an investigative CRA. Structurally distinct from Eightfold's scraped-profile model. Legal review required pre-launch. |
| CCPA | Deletion honored within 72h. No data sale. No data broking. |
| Illinois BIPA | No biometric data. No facial or voice analysis. |

---

## 6. Product Architecture

```
TOOL ECOSYSTEM (MCP Clients)
Claude · Cursor · Copilot · Windsurf · Gemini · Any MCP Tool
          ↕ prism://skills/* (localhost:7371)
┌──────────────────────────────────────────────────────────┐
│  ON-DEVICE                                               │
│  ┌─────────────────────────────────────────────────┐    │
│  │  MCP SERVER (localhost:7371)                    │    │
│  │  Consent Gate → Skill Graph Reader              │    │
│  │  → Query-as-Signal Emitter                      │    │
│  └────────────────┬────────────────────────────────┘    │
│                   │ IPC (no network socket)              │
│  ┌────────────────▼────────────────────────────────┐    │
│  │  LAYER 1 — PERSONAL MEMORY AGENT                │    │
│  │  Local LLM · Encrypted Skill Graph · Buffer     │    │
│  └────────────────┬────────────────────────────────┘    │
│                   │                                      │
│  ┌────────────────▼────────────────────────────────┐    │
│  │  DP LAYER + ATTESTATION                         │    │
│  │  Noise injection · Device signing · Purge       │    │
│  └────────────────┬────────────────────────────────┘    │
│                   │ Weekly user-approved upload only     │
└───────────────────┼──────────────────────────────────────┘
        RAW DATA NEVER CROSSES THIS BOUNDARY
                    │
┌───────────────────▼──────────────────────────────────────┐
│  PRISM SERVERS                                           │
│  Layer 2: Signal Extraction Engine                       │
│  Layer 3: Behavioral Talent Profile                      │
│  Layer 4: Employer Discovery Engine                      │
└──────────────────────────────────────────────────────────┘
```

### MCP Server (localhost:7371)

The MCP server is a first-class process in the Prism desktop client (Tauri/Rust). It exposes JSON-RPC over stdio or HTTP, MCP spec compliant, discoverable by tools via standard MCP registry.

**Endpoints:**

| Endpoint | Returns | Query-as-Signal |
|---|---|---|
| `prism://skills` | Full skill vector across all domains | Broad context request → general assistant session |
| `prism://skills/{domain}` | Depth + recency for specific domain | Domain query → active work in that area right now |
| `prism://context/current` | Current work mode, stack, collab level | Context query → session type classification |
| `prism://preferences` | Work style and communication preferences | Preference query → active AI collab session |
| `prism://collab-level` | Current AI collab level (1–5) + sub-scores | Collab query → autonomy signal for this task |
| `prism://signal` (write) | Tool writes explicit outcome signal back | Highest quality signal — structured, outcome-confirmed |

The MCP server and passive agent share the encrypted local skill graph via IPC. No network socket. Same process boundary. No new attack surface.

**Per-Tool Consent Matrix (default shown, user-configurable):**

| Tool | skills/* | context | preferences | collab-level | signal (write) |
|---|---|---|---|---|---|
| Claude | ✅ | ✅ | ✅ | ✅ | ✅ |
| Cursor | ✅ | ✅ | ❌ | ✅ | ✅ |
| Copilot | ✅ | ✅ | ❌ | ❌ | ❌ |
| Gemini | ✅ | ❌ | ✅ | ❌ | ❌ |
| Custom | ⚙ | ⚙ | ⚙ | ⚙ | ⚙ |

### Layer 1 — Personal Memory Agent (On-Device)

Passive capture via browser extension and tool connectors. Feeds the same encrypted local skill graph as the MCP server. Tool connectors are metadata-only — session completion events, commit timestamps, task-done signals. No content.

Memory architecture: temporal knowledge graph of skill nodes and evidence edges (SQLite + FAISS index). Encrypted at rest. Never transmitted directly. The MCP server reads from this graph; the inference engine writes to it.

### Layer 2 — Signal Extraction Engine (Server-Side)

Receives DP-noised, user-approved signal uploads. Performs:
- ESCO taxonomy mapping (canonical skill ontology + AI-native extension)
- Role archetype scoring against job posting corpora
- Skill velocity modeling (accelerating, plateauing, decaying)
- AI collaboration typology classification (Level 1–5 across five sub-dimensions)

### Layer 3 — Behavioral Talent Profile

Three visibility states: **Private**, **Discoverable**, **Open**.

Profile components:
- **Skill Constellation** — visual map of skill domains with depth and recency
- **AI Collaboration Score** — 0–100 across Delegation, Verification, Iteration, System Design, Tool Diversity
- **Work Style Narrative** — 3–5 sentence AI-generated paragraph derived from signal data; user-reviewed and edited before publishing
- **Role Affinity Scores** — ranked list of role archetypes the profile matches
- **Evidence Anchors** — user-curated outcome summaries, opt-in per item
- **Trajectory** — skill growth timeline over active Prism period

### Layer 4 — Employer Discovery Engine

Natural language query → NL parser → structured filter object → vector search (Pinecone/Weaviate) → filter layer → fit score engine → ranked results.

Fit score weights: Skill Match 35%, AI Collab Level 25%, Skill Velocity 20%, Role Affinity 15%, Recency 5%. Weights tunable per employer using their own historical hire outcomes.

---

## 7. AI Collaboration Taxonomy

Prism's proprietary classification of how people work with AI — the dimension no existing platform measures.

| Level | Label | Description |
|---|---|---|
| 1 | Prompt User | Reactive, one-off tasks. Single-turn, generic prompts, no iteration. |
| 2 | Workflow Integrator | Embeds AI into repeatable processes. Multi-step, consistent tooling, prompt reuse. |
| 3 | Verification Practitioner | Critically evaluates and corrects AI outputs. High edit rates, error detection patterns. |
| 4 | Agent Orchestrator | Coordinates multiple AI agents/tools toward a goal. Multi-tool sessions, pipeline design. |
| 5 | System Designer | Architects AI-native systems for others. Documentation signals, teaching patterns, toolchain architecture. |

---

## 8. Business Model

### B2C (Candidate Side)

| Tier | Price | Features |
|---|---|---|
| Free | $0 | MCP server + local skill graph, private profile, basic skill map |
| Career | $12/mo | Discoverable profile, role affinity scores, trajectory, AI-generated narrative |
| Pro | $29/mo | Full analytics, employer interest feed, application co-pilot, skill gap recommendations |

The free tier includes the full MCP server. This is deliberate: the MCP server is the acquisition channel. Users install Prism to make their AI tools smarter. Hiring is secondary until they're ready for it.

### B2B (Employer Side)

| Tier | Price | Target |
|---|---|---|
| Starter | $299/mo | 3 recruiter seats, 100 profile views/mo, basic search |
| Growth | $899/mo | 10 seats, unlimited views, NL search, outreach tools |
| Enterprise | Custom | Unlimited seats, ATS integrations, custom role models, dedicated CSM |

LinkedIn Recruiter Corporate seats run $900–$1,080/seat/month. Prism's Growth tier matches that price point at comparable seat count but with behavioral signal depth LinkedIn is architecturally incapable of providing.

**Additional revenue streams:**
- **Skill verification API** — bootcamps and learning platforms verify graduate outcomes against Prism skill signals. SaaS API pricing.
- **Employer role modeling** — custom AI collaboration archetype definition for specific roles. Professional services, $5–15K per engagement.
- **Workforce intelligence reports** — anonymized aggregate skill trend data for L&D teams and analysts. Annual subscription.
- **Tool developer API** — third-party AI tools pay for Prism MCP server certification and featured placement in the tool registry. Small but high-signal revenue.

---

## 9. Go-To-Market Strategy

### Phase 1: Tool Utility (Months 1–6) — Supply Side

**Lead with the MCP server, not the hiring platform.**

The pitch to early adopters is not "build your talent profile." It is: "Install Prism and your AI tools will know who they're talking to." This is genuinely useful, immediately, with no hiring intent required. Claude stops explaining beginner Python to a senior engineer. Cursor adjusts its suggestions to your actual working style. This is the wedge.

**Target early adopters:**
- Developers actively using Cursor, Claude, or Copilot who have felt the friction of starting every session cold
- AI practitioners and agent engineers (highest signal density, community influence)
- Bootcamp/course graduates who want to evidence skills they've built
- Power users proud of how they use AI and willing to share that story

**Distribution:**
- Product Hunt, Hacker News, AI Engineer World's Fair, LangChain/LlamaIndex Discord communities
- Integrations partnerships with Cursor, Notion, Linear for featured placement
- University career center partnerships for new-grad AI practitioners

### Phase 2: Demand Activation (Months 4–9) — Employer Side

Once 5,000+ quality profiles are live, open employer access on a pilot basis.

Target first employer cohort: AI-native startups (Series A–C) actively hiring ML engineers and AI product managers; staffing firms specializing in AI talent; mid-market tech companies frustrated with LinkedIn for technical AI roles.

**Pilot structure:** 90-day paid pilot, monthly billing, cancel anytime. Measure: candidates surfaced, quality-of-hire score (collected post-hire), time-to-hire reduction.

### Phase 3: Ecosystem Moat (Month 9+)

Each hire produces outcome data that refines role models. Better matches → more hires → more outcome data → better models → better matches. The three-sided network effect (users ↔ tools ↔ employers) compounds faster than a two-sided talent marketplace.

**Tool developer program:** Open the MCP server spec and invite AI tool developers to build native Prism integrations. Each new tool that calls `prism://skills` is a new distribution channel and a new signal source.

---

## 10. Technical Stack

| Component | Choice | Rationale |
|---|---|---|
| Desktop client | Tauri (Rust) | Small binary, native OS access, process-level controls |
| MCP server | Rust (within Tauri) | Same process boundary as agent; no network socket needed |
| On-device LLM | Ollama + Phi-4-mini | Small enough for background inference; fine-tunable for skill extraction |
| Browser extension | Manifest V3 | Hooks session completion events; no content access |
| Local memory | SQLite (encrypted) + FAISS | Lightweight, local-only, shared between MCP server and agent via IPC |
| Signal transmission | gRPC + TLS 1.3 | Binary protocol; schema-enforced at gateway |
| Backend | FastAPI (Python) + Go services | Python for ML pipeline; Go for latency-critical query paths |
| Vector DB | Pinecone or Weaviate | Employer-side semantic search |
| Skill taxonomy | ESCO + custom AI-native extension | ESCO is EU standard; extension covers LLM, agentic, prompt engineering skills |
| Auth | Clerk | Standard; nothing custom |
| Differential privacy | Apple DP library (open source) | Battle-tested; Apache 2.0 license |
| Attestation | TPM 2.0 + device fingerprint | Hardware-backed signing |

---

## 11. Risks and Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Cold start — not enough candidates | High | Phase 1 leads with MCP utility. Hiring is secondary. Tool value requires zero social graph. |
| Gaming / profile fabrication | High | Attestation + consistency scoring. Long-term trajectory cannot be easily spoofed. |
| Employer distrust of behavioral signals | Medium | Explainability layer on fit scores. Pilot outcome data builds credibility over time. |
| EU AI Act high-risk (employment) classification | Medium | User-permissioned, user-reviewed, human oversight on all hiring decisions. DPIA template at onboarding. |
| On-device inference quality | Medium | Narrow scope: skill extraction only. Expand post-PMF. |
| "Surveillance" perception backlash | High | MCP pull model is the answer. Tools ask Prism for context — Prism doesn't push. That's a fundamentally cleaner privacy story than passive observation. |
| FCRA / CRCA classification (US) | Medium | User-originated, user-attested data. Pre-launch legal review required. |
| MCP spec changes | Low | Prism controls its own server implementation. MCP is open; Prism is not dependent on a single vendor's MCP client behavior. |
| LinkedIn building a local agent | Medium | LinkedIn's moat requires surveillance advertising. A local-first, privacy-by-design architecture directly conflicts with their business model. Structurally difficult to replicate. |
| Tool developers bypassing Prism for direct user context | Low-Medium | First-mover advantage + tool registry program. Being the canonical skills context provider requires network effects Prism builds in Phase 1. |

---

## 12. MVP Scope

The MVP proves one thesis: **a local MCP server serving skill context to AI tools, with a passive profile-building side effect, is something people install and keep installed.**

**MVP deliverables:**

1. **Prism Agent + MCP Server (desktop app)** — MCP server exposing `prism://skills` and `prism://context/current`. Passive capture via Claude.ai and ChatGPT browser extension hooks. Local inference on session completion. Encrypted local skill graph.

2. **Private Dashboard** — Web UI showing the user their own skill constellation and AI collaboration score. No employer access. Pure personal utility.

3. **Weekly Digest** — Email or in-app summary of extracted signals. User approves or discards. Nothing transmits without approval.

4. **One Employer Design Partner** — Manual access to 50 candidate profiles. Structured feedback on signal quality.

**MVP success criteria:**
- 500 active agents (weekly digest generated)
- 70%+ of users report MCP context noticeably improves their tool responses
- 70%+ report the weekly digest is useful independent of job seeking
- Design partner rates 80%+ of surfaced profiles as "would interview"
- Zero privacy incidents or raw data exposure events

**MVP explicitly excludes:** Employer search portal, payments, attestation system, multi-tool connectors beyond Claude/ChatGPT, role-fit scoring, outreach features, write endpoint, full consent matrix UI.

---

## 13. Key Resolved Questions

**Q: How does multi-device work?**
Signal uploads are keyed to device identity. Multi-device users get merged profiles via server-side additive aggregation of skill vectors — no signal is attributed to a specific device in the merge output.

**Q: What happens on account deletion?**
Server-side deletion completes within 72 hours. Local agent purged via secure-delete through the app's reset function. Employer-cached data invalidated within 24 hours via access token revocation cascade. Deletion receipt emailed to user.

**Q: Can employers or governments subpoena raw data?**
Prism cannot produce raw activity data. It was never stored. A subpoena yields only the same derived vectors the employer already accessed — same as what the user sees in their own dashboard.

**Q: How do sensitive/NDA-covered work contexts work?**
Capture consent defaults to off. Private Mode (OS process suspend) kills both the passive agent and MCP server instantly. Onboarding explicitly advises against enabling capture in NDA or security-cleared contexts.

**Q: What is the moat if LinkedIn copies this?**
LinkedIn's advertising business model requires behavioral surveillance. A local-first, no-raw-data architecture directly conflicts with that model. They are incentivized in the opposite direction. Eightfold's January 2026 FCRA lawsuit is the early signal of where the incumbent model is heading legally. Prism's moat is the thing LinkedIn cannot build without dismantling itself.

**Q: What if an AI tool decides to build their own persistent user context?**
Individual tools building their own context stores creates fragmentation — a user's Cursor context doesn't help their Claude session. Prism's value is being the single shared context layer all tools query. First-mover advantage on the MCP registry and tool developer program is the defense.

---

*Prism v1.1 Spec — Confidential Working Document*
*Next artifacts: DPIA template · Employer pilot outreach deck · MCP server implementation spec*
