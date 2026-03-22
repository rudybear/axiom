# AXIOM Agentic Development Pipeline

## Overview

A multi-agent CI pipeline where independent AI agents handle architecture, coding, review, testing, and benchmarking — with git-based diffs for verification and automatic retry loops on failure.

## State Machine

```
                    ┌──────────────────────────────────┐
                    │                                  │
                    v                                  │
┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐
│ ARCHITECT│──>│  CODER   │──>│ REVIEWER │──>│  TESTER  │──>│BENCHMARK │──> MERGE
└──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘
     ^              ^              │               │               │
     │              │              │               │               │
     │              └──────────────┘               │               │
     │            REQUEST_CHANGES                  │               │
     │                                             │               │
     └─────────────────────────────────────────────┘               │
                      REJECT / FAIL                                │
     └─────────────────────────────────────────────────────────────┘
                    REGRESSION (loops back to coder)
```

## Quick Start

```bash
# List milestones
.pipeline/scripts/run-pipeline.sh --list

# Run a milestone through the full pipeline
.pipeline/scripts/run-pipeline.sh M1.1-lexer

# Check pipeline status
.pipeline/scripts/run-pipeline.sh --status

# Run gate check only
.pipeline/scripts/run-pipeline.sh --gate M1.1-lexer
```

## Agents

| Agent | Role | Branch | Output |
|-------|------|--------|--------|
| **Architect** | Design specs, API signatures, acceptance criteria | `architect/{run}/{milestone}` | `architect-output.json` |
| **Coder** | Implement Rust code from spec | `coder/{run}/{milestone}` | `coder-output.json` |
| **Reviewer** | Adversarial code review against spec + conventions | (none) | `reviewer-output.json` |
| **Tester** | Run tests, verify acceptance criteria, add edge cases | `tester/{run}/{milestone}` | `tester-output.json` |
| **Benchmark** | Measure performance, detect regressions | (none) | `benchmark-output.json` |

## Milestones

| ID | Name | Depends On | Acceptance Criteria |
|----|------|------------|-------------------|
| M1.1-lexer | Lexer | — | 8 criteria |
| M1.2-parser | Parser | M1.1 | 10 criteria |
| M1.3-hir | HIR | M1.2 | 8 criteria |
| M1.4-codegen | LLVM Codegen | M1.3 | 8 criteria |
| M1.5-e2e | End-to-End | M1.4 | 8 criteria |

## Retry Policy

- **Coder** retries: max 3 (on reviewer REQUEST_CHANGES or test failure)
- **Architect** retries: max 2 (on reviewer REJECT or coder exhaustion)
- **Reviewer** cycles: max 3 (before escalating)

## Directory Structure

```
.pipeline/
├── PIPELINE.md              # This file
├── config.json              # Pipeline configuration
├── milestones/              # Milestone definitions with acceptance criteria
│   ├── M1.1-lexer.json
│   ├── M1.2-parser.json
│   ├── M1.3-hir.json
│   ├── M1.4-codegen.json
│   └── M1.5-e2e.json
├── templates/               # Agent system prompts
│   ├── architect.md
│   ├── coder.md
│   ├── reviewer.md
│   ├── tester.md
│   └── benchmark.md
├── scripts/                 # Pipeline automation
│   ├── run-pipeline.sh      # Convenience runner
│   ├── orchestrator.sh      # Main state machine
│   ├── run-architect.sh     # Agent launchers
│   ├── run-coder.sh
│   ├── run-reviewer.sh
│   ├── run-tester.sh
│   ├── run-benchmark.sh
│   ├── gate-check.sh        # Acceptance criteria verifier
│   └── rollback.sh          # Failure recovery
├── benchmarks/              # Performance baselines and history
│   ├── baselines.json
│   ├── regression-config.json
│   └── {milestone}.jsonl    # Historical benchmark data
└── runs/                    # Per-execution state (gitignored)
    └── run-YYYYMMDD-NNN/
        ├── state.json
        ├── log.jsonl
        ├── *-output.json
        └── *-prompt.md
```

## Handoff Protocol

Every agent produces a JSON file with this envelope:

```json
{
  "pipeline_version": "1.0",
  "run_id": "run-20260321-001",
  "milestone_id": "M1.2-parser",
  "agent": "architect",
  "timestamp": "2026-03-21T14:30:00Z",
  "git_sha": "abc1234...",
  "status": "complete",
  ...agent-specific payload...
}
```

## Git Workflow

- `main` is protected — only updated by successful pipeline merges
- Each agent works on its own branch: `{agent}/{run-id}/{milestone-id}`
- Merge uses `--no-ff` to preserve milestone boundaries in history
- Failed runs leave branches intact for post-mortem inspection
