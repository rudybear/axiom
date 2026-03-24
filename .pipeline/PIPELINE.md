# AXIOM Agentic Development Pipeline v2.0

## Overview

A 7-agent development pipeline with **design review before coding** and **dual code review after coding**. Plans are validated by optimistic and pessimistic reviewers before implementation begins. Test requirements are agreed during design, verified by QA.

## Pipeline State Machine

```
Phase 1: DESIGN
═══════════════════════════════════════════════════════════
                    ┌──────────────┐
                    │  ARCHITECT   │ Designs spec + plan
                    └──────┬───────┘
                           │
              ┌────────────▼────────────────┐
              │     DESIGN REVIEW           │
              │  ┌──────────┐ ┌──────────┐  │
              │  │OPTIMISTIC│ │PESSIMISTIC│  │
              │  │"Will work│ │"What goes │  │
              │  │ because" │ │ wrong?"   │  │
              │  └────┬─────┘ └────┬──────┘  │
              └───────┼────────────┼─────────┘
                      │  feedback  │
                      └─────┬──────┘
                            │ if issues
                   ┌────────▼────────┐
                   │ ARCHITECT revise│ ← loop until both APPROVE
                   └────────┬────────┘
                            │ agreed plan
                            │ (saved to docs, DESIGN.md updated)
                            ▼

Phase 2: IMPLEMENTATION
═══════════════════════════════════════════════════════════
                   ┌─────────────────┐
                   │     CODER       │ Implements agreed plan
                   └────────┬────────┘
                            │
                   ┌────────▼────────┐
                   │    QA AGENT     │ Verifies tests match
                   │                 │ agreed requirements
                   └────────┬────────┘
                            │

Phase 3: CODE REVIEW
═══════════════════════════════════════════════════════════
              ┌────────────▼────────────────┐
              │     CODE REVIEW             │
              │  ┌──────────┐ ┌──────────┐  │
              │  │OPTIMISTIC│ │PESSIMISTIC│  │
              │  │"Matches  │ │"Any UB?  │  │
              │  │ spec"    │ │ Races?"  │  │
              │  └────┬─────┘ └────┬──────┘  │
              └───────┼────────────┼─────────┘
                      │            │
                      └─────┬──────┘
                            │
              COMPLETE ─────┤──── ISSUES
                  │                  │
                  ▼                  ▼
              MERGE to        Back to ARCHITECT
              master          (not coder — design
                              may need revision)
```

## The 7 Agents

| # | Agent | Phase | Role |
|---|-------|-------|------|
| 1 | **Architect** | Design | Designs specification, execution plan, test requirements |
| 2 | **Optimistic Design Reviewer** | Design Review | Validates against existing solutions, AXIOM goals, feasibility |
| 3 | **Pessimistic Design Reviewer** | Design Review | Finds correctness holes, principle violations, edge cases, UB risks |
| 4 | **Coder** | Implementation | Implements the agreed plan exactly |
| 5 | **QA Agent** | Verification | Verifies tests conform to agreed requirements, coverage completeness |
| 6 | **Optimistic Code Reviewer** | Code Review | Verifies implementation matches spec, patterns, quality |
| 7 | **Pessimistic Code Reviewer** | Code Review | Finds bugs, UB, race conditions, performance issues |

## Key Principles

1. **No coding before design approval** — Both design reviewers must APPROVE before the coder starts
2. **Agreed plan is saved** — Design decisions documented in `docs/` and `DESIGN.md` before coding
3. **Test requirements negotiated upfront** — Between architect and reviewers, not invented by coder
4. **Issues go to architect, not coder** — If code review finds design problems, the architect revises
5. **QA verifies conformance** — Tests must match what was agreed, not just "something runs"

## Retry Policy

| Situation | Action |
|-----------|--------|
| Design reviewer NEEDS_REVISION | Architect revises, re-submit to both reviewers |
| Design reviewer REJECT | Architect fundamentally redesigns |
| QA FAIL (missing tests) | Coder adds missing tests |
| Code reviewer REQUEST_CHANGES | Coder fixes (minor) or Architect revises (design issue) |
| Code reviewer REJECT | Back to Architect — the plan was wrong |
| Max design review cycles: 3 | Escalate to human |
| Max code review cycles: 2 | Escalate to human |

## Directory Structure

```
.pipeline/
├── PIPELINE.md              # This file
├── config.json              # Pipeline v2.0 configuration
├── milestones/              # Milestone definitions
├── templates/               # Agent system prompts (7 agents)
│   ├── architect.md
│   ├── optimistic_design_reviewer.md
│   ├── pessimistic_design_reviewer.md
│   ├── coder.md
│   ├── qa_agent.md
│   ├── optimistic_code_reviewer.md
│   ├── pessimistic_code_reviewer.md
│   ├── tester.md             # (legacy, retained for reference)
│   ├── reviewer.md           # (legacy, retained for reference)
│   └── benchmark.md          # (legacy, retained for reference)
├── scripts/                 # Pipeline automation
├── benchmarks/              # Performance baselines
└── runs/                    # Per-execution state (gitignored)
```

## Master Task List

See `docs/MASTER_TASK_LIST.md` for the complete reordered work plan (47 milestones across 8 tracks).
